use clap::Parser;
use xmpp_parsers::jid::{BareJid, Jid};

/// A fictional versioning CLI
#[derive(Debug, Parser)] // requires `derive` feature
#[command(name = "esmart_mqtt")]
#[command(about = "eSmart - MQTT bridge", long_about = None)]
pub(crate) struct Cli {
    // XMPP Server settings
    #[arg(long, value_name = "JID", env)]
    pub(crate) xmpp_jid: Jid,
    #[arg(long, value_name = "PASSWORD", env)]
    pub(crate) xmpp_password: String,
    #[arg(long, value_name = "ROOM@CONFERENCE.XMPP-SERVER.COM", env)]
    pub(crate) xmpp_room: BareJid,

    // MQTT Server settings
    #[arg(long, value_name = "HOST", env)]
    pub(crate) mqtt_hostname: String,
    #[arg(long, value_name = "USERNAME", env)]
    pub(crate) mqtt_username: Option<String>,
    #[arg(long, value_name = "PASSWORD", env)]
    pub(crate) mqtt_password: Option<String>,

    // MQTT Options
    #[arg(long, value_name = "ID", env, default_value = "esmarter")]
    pub(crate) mqtt_id: String,
    #[arg(long, value_name = "PORT", env, default_value_t = 8113)]
    pub(crate) mqtt_port: u16,
    #[arg(long, value_name = "SECONDS", env, default_value_t = 300)]
    pub(crate) mqtt_throttling_secs: u32,

    // XMPP Options
    #[arg(long, value_name = "NICKNAME", env, default_value = "esmart")]
    pub(crate) xmpp_nickname: String,
}
