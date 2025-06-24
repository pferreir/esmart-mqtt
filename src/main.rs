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
use tokio_xmpp::{Client, Error, Event};
use xmpp_parsers::{
    message::Message,
    muc::Muc,
    presence::{Presence, Show as PresenceShow, Type as PresenceType},
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

    log::debug!("Sending discovery message to '{}': {}", config_topic, json);
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

    log::debug!("Sending data payload to '{}': {}", topic, json);
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

    let mut client = Client::new(
        tokio_xmpp::jid::BareJid::new(&args.xmpp_jid.to_string())?,
        args.xmpp_password.clone(),
    );

    while let Some(elem) = client.next().await {
        match elem {
            Event::Stanza(stanza) => match Message::try_from(stanza) {
                Ok(message) => {
                    if let Some((_, body)) = message.get_best_body(vec![""]) {
                        match serde_json::from_str::<data::ESmartMessage>(body) {
                            Ok(msg) => {
                                messages_recv.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
                                msg.iter_meters()
                                    .for_each(|meter| process_meter(&mut sender, meter));
                                msg.iter_nodes()
                                    .for_each(|node| process_node(&mut sender, node));
                            }
                            Err(e) => log::warn!("Failed to parse ESmartMessage: {:?}", e),
                        }
                    }
                }
                Err(_) => continue,
            },
            Event::Disconnected(e) => {
                log::error!("XMPP client disconnected: {:?}", e);
                return Err(Error::Disconnected);
            }
            Event::Online { .. } => {
                let mut presence = Presence::new(PresenceType::None);
                presence.show = Some(PresenceShow::Chat);
                client.send_stanza(presence.into()).await?;

                let muc = Muc::new();
                let room_jid = args
                    .xmpp_room
                    .with_resource_str(&args.xmpp_nickname)
                    .unwrap();
                let mut presence = Presence::new(PresenceType::None).with_to(room_jid);
                presence.add_payload(muc);
                presence.set_status("en", "here");
                let _ = client.send_stanza(presence.into()).await;
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
        match &args.mqtt_password {
            Some(password) => {
                options.set_credentials(username, password);
            }
            None => {
                log::error!("MQTT username is set but password is missing!");
                return;
            }
        }
    }

    options.set_keep_alive(Duration::from_secs(5));

    let (client, mut event_loop) = AsyncClient::new(options, 10);

    let mqtt_throttling_secs = args.mqtt_throttling_secs as i64;

    // spawn a task to handle incoming messages from the XMPP client
    let mut client_clone = client.clone();
    let receiver_handle = tokio::spawn(async move {
        loop {
            match receiver.recv().await {
                Ok((id, stat, value)) => {
                    log::debug!("Processing update for {id}");

                    match cache_queues.get_mut(&id) {
                        Some(queue) => queue.push(value),
                        None => {
                            if let Err(e) = send_mqtt_discovery(&mut client_clone, &id, &stat).await
                            {
                                log::error!("Error sending discovery message: {:?}", e);
                            }
                            cache_queues.insert(id.clone(), Vec::new());
                        }
                    }

                    let send_message = match last_contact.get(&id) {
                        Some(ts) => {
                            Utc::now() > (*ts + ChronoDuration::seconds(mqtt_throttling_secs))
                        }
                        None => true,
                    };

                    if send_message {
                        let queue = cache_queues.get_mut(&id).unwrap();
                        let avg = if !queue.is_empty() {
                            queue.iter().fold(0.0, |acc, e| acc + *e) / queue.len() as f32
                        } else {
                            0.0
                        };

                        queue.clear();
                        let _ = last_contact.insert(id.clone(), Utc::now());
                        if let Err(e) =
                            send_mqtt_update(&mut client_clone, &id, stat.property(), avg).await
                        {
                            log::error!("Error sending update message: {:?}", e);
                        } else {
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

    // Main MQTT event loop: exit on error
    loop {
        match event_loop.poll().await {
            Ok(notification) => {
                log::trace!("Received = {:?}", notification);
            }
            Err(e) => {
                log::error!("MQTT event loop error: {:?}", e);
                break;
            }
        }
    }

    // If we exit the event loop, abort the receiver task and return
    receiver_handle.abort();
    log::warn!("MQTT event loop exited, task will return for reconnect.");
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

    let args_clone = args.clone();
    let receiver_clone = receiver.resubscribe();
    let messages_sent_clone = messages_sent.clone();

    // Spawn XMPP task
    let xmpp_handle = tokio::spawn(async move {
        loop {
            log::info!("Starting XMPP task");
            xmpp_task(sender.clone(), args_clone.clone(), messages_recv.clone())
                .await
                .unwrap_or_else(|e| {
                    log::error!("XMPP task failed: {:?}", e);
                });
            // Wait 5s before trying to reconnect
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });

    // Spawn MQTT task with reconnect loop
    let mqtt_handle = tokio::spawn(async move {
        loop {
            log::info!("Starting MQTT task");
            mqtt_task(
                receiver_clone.resubscribe(),
                args.clone(),
                messages_sent_clone.clone(),
            )
            .await;
            log::warn!("MQTT task failed or disconnected, reconnecting in 5s...");
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });

    // Wait for either task to finish
    tokio::select! {
        _ = xmpp_handle => {
            log::warn!("XMPP task exited, shutting down.");
        }
        _ = mqtt_handle => {
            log::warn!("MQTT task exited, shutting down.");
        }
    }
}
