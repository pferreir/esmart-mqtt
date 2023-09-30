use std::{
    collections::HashMap,
    sync::{atomic::AtomicU64, Arc},
    time::Duration,
};

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use clap::Parser;
use data::{Meter, Node};
use rumqttc::{AsyncClient, ClientError, MqttOptions};
use serde_json::json;
use stats::{IterStats, Stat};
use tokio::sync::broadcast::{self, Receiver, Sender};
use tokio_stream::StreamExt;
use tokio_xmpp::{Error, SimpleClient as Client};
use xmpp_parsers::{
    message::Message,
    muc::Muc,
    presence::{Presence, Show as PresenceShow, Type as PresenceType},
    Jid,
};

mod cli;
mod data;
mod stats;

type ChannelMessage = (String, Stat, f32);

fn process_meter(queue: &mut Sender<ChannelMessage>, meter: &Meter) {
    for (id, stat, value) in meter.into_stats_iter() {
        match queue.send((id, stat, value)) {
            Ok(_) => {}
            Err(_) => log::error!("This shouldn't happen!"),
        }
    }
}

fn process_node(queue: &mut Sender<ChannelMessage>, node: &Node) {
    for (id, stat, value) in node.into_stats_iter() {
        match queue.send((id, stat, value)) {
            Ok(_) => {}
            Err(_) => log::error!("This shouldn't happen!"),
        }
    }
}

async fn send_mqtt_discovery(
    client: &mut AsyncClient,
    id: &str,
    stat: &Stat,
) -> Result<(), ClientError> {
    let topic = format!("homeassistant/sensor/esmart/{id}");

    let config_topic = format!("{topic}/config");
    let state_topic = format!("esmart/{id}/state");
    let name = format!("{} ({})", stat.name(), stat.property());

    let json = json!({
        "name": name,
        "state_topic": state_topic,
        "device_class": stat.device_class_str(),
        "icon": stat.icon(),
        "value_template": format!("{{{{ value_json.{} }}}}", stat.property()),
        "unit_of_measurement": stat.unit_str(),
        "unique_id": format!("esmart_{id}"),
        "device": {
            "name": name,
            "model": "eSmarter Client",
            "identifiers": [format!("esmart_{id}")]
        }
    });

    log::debug!(
        "Sending discovery message to '{}': {}",
        config_topic,
        json.to_string()
    );
    client
        .publish(
            config_topic,
            rumqttc::QoS::AtLeastOnce,
            true,
            json.to_string(),
        )
        .await?;

    Ok(())
}

async fn send_mqtt_update(
    client: &mut AsyncClient,
    id: &str,
    property: &str,
    value: f32,
) -> Result<(), ClientError> {
    let topic = format!("esmart/{id}/state");
    let json = json!({ property: value });

    log::debug!("Sending data payload to '{}': {}", topic, json.to_string());
    client
        .publish(topic, rumqttc::QoS::AtLeastOnce, false, json.to_string())
        .await?;

    Ok(())
}

async fn xmpp_task(
    mut sender: Sender<ChannelMessage>,
    args: Arc<cli::Cli>,
    messages_recv: Arc<AtomicU64>,
) -> Result<(), Error> {
    log::info!("Started XMPP task");

    let mut client = Client::new_with_jid(args.xmpp_jid.clone(), args.xmpp_password.clone())
        .await
        .expect("could not connect to xmpp server");

    let mut presence = Presence::new(PresenceType::None);
    presence.show = Some(PresenceShow::Chat);
    client.send_stanza(presence).await?;

    let muc = Muc::new();
    let room_jid = args.xmpp_room.with_resource_str(&args.xmpp_nickname)?;
    let mut presence = Presence::new(PresenceType::None).with_to(Jid::Full(room_jid));
    presence.add_payload(muc);
    presence.set_status("en", "here");
    let _ = client.send_stanza(presence).await;

    while let Some(v) = client.next().await {
        let elem = v?;
        let mut buf = Vec::new();

        elem.write_to(&mut buf).unwrap();

        if let Ok(message) = Message::try_from(elem) {
            if let Some((_, body)) = message.get_best_body(vec![""]) {
                if let Ok(msg) = serde_json::from_str::<'_, data::ESmartMessage>(&body.0) {
                    messages_recv.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
                    for meter in msg.iter_meters() {
                        process_meter(&mut sender, meter);
                    }
                    for node in msg.iter_nodes() {
                        process_node(&mut sender, node);
                    }
                }
            }
        }
    }

    Ok(())
}

// task which fetches the stats from the queue and sends them to MQTT
async fn mqtt_task(
    mut receiver: Receiver<ChannelMessage>,
    args: Arc<cli::Cli>,
    messages_sent: Arc<AtomicU64>,
) {
    let mut last_contact: HashMap<String, DateTime<Utc>> = HashMap::new();
    let mut cache_queues: HashMap<String, Vec<f32>> = HashMap::new();

    let mut options = MqttOptions::new(&args.mqtt_id, &args.mqtt_hostname, args.mqtt_port);

    log::info!("Started MQTT task");

    if let Some(username) = &args.mqtt_username {
        options.set_credentials(username, args.mqtt_password.as_ref().unwrap());
    }

    options.set_keep_alive(Duration::from_secs(5));

    let (mut client, mut event_loop) = AsyncClient::new(options, 10);

    let mqtt_throttling_secs = args.mqtt_throttling_secs as i64;

    tokio::spawn(async move {
        loop {
            match receiver.recv().await {
                Ok((id, stat, value)) => {
                    log::debug!("Processing update for {id}");

                    match cache_queues.get_mut(&id) {
                        Some(queue) => queue.push(value),
                        None => {
                            // if there is no cache queue yet for a key, this means we've just got to know this stat exists
                            // then, advertise it to Home Assistant
                            if let Err(e) = send_mqtt_discovery(&mut client, &id, &stat).await {
                                log::error!("Error sending discovery message: {:?}", e);
                            }
                            // create a new cache queue to keep track of the values
                            cache_queues.insert(id.clone(), Vec::new());
                        }
                    }

                    // check whether we've sent a value recently
                    let send_message = match last_contact.get(&id) {
                        Some(ts) => {
                            Utc::now() > (*ts + ChronoDuration::seconds(mqtt_throttling_secs))
                        }
                        None => true,
                    };

                    if send_message {
                        // we can safely unwrap here, since we've already made sure `id` exists in `cache_queues` above
                        let queue = cache_queues.get_mut(&id).unwrap();
                        let avg = queue.iter().fold(0.0, |acc, e| acc + *e) / queue.len() as f32;

                        // reset cache queue
                        queue.clear();
                        let _ = last_contact.insert(id.clone(), Utc::now());
                        if let Err(e) =
                            send_mqtt_update(&mut client, &id, stat.property(), avg).await
                        {
                            log::error!("Error sending update message: {:?}", e);
                        } else {
                            // increase counter
                            messages_sent.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
                        }
                    }
                }
                Err(broadcast::error::RecvError::Closed) => {
                    log::error!("XMPP half seems to have died!");
                    return;
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    log::error!("MQTT receiver lagging behind!");
                }
            }
        }
    });

    while let Ok(notification) = event_loop.poll().await {
        log::trace!("Received = {:?}", notification);
    }
}

#[tokio::main]
async fn main() {
    let args = Arc::new(cli::Cli::parse());

    env_logger::init();

    let (sender, receiver) = broadcast::channel(256);

    let messages_sent = Arc::new(AtomicU64::new(0));
    let messages_recv = Arc::new(AtomicU64::new(0));

    let ms = messages_sent.clone();
    let mr = messages_recv.clone();
    // task to report statistics every now and then
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            log::info!(
                "Messages: {} received / {} sent",
                mr.load(std::sync::atomic::Ordering::Acquire),
                ms.load(std::sync::atomic::Ordering::Acquire)
            );
        }
    });

    tokio::spawn(xmpp_task(sender, args.clone(), messages_recv));
    tokio::join!(mqtt_task(receiver, args, messages_sent));
}
