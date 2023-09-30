use crate::stats::{DeviceClass, IterStats, Stat, Unit};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

mod date_format {
    use chrono::{DateTime, Utc};
    use serde::{self, Deserialize, Deserializer, Serializer};

    const FORMAT: &str = "%Y-%m-%d %H:%M:%S%z";

    #[allow(dead_code)]
    pub fn serialize<S>(date: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = format!("{}", date.format(FORMAT));
        serializer.serialize_str(&s)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(DateTime::parse_from_str(&s, FORMAT).unwrap().into())
    }
}

#[derive(Deserialize, Debug, Clone)]
pub enum OnOff {
    #[serde(rename = "on")]
    On,
    #[serde(rename = "off")]
    Off,
}

impl From<&OnOff> for bool {
    fn from(value: &OnOff) -> bool {
        match value {
            OnOff::On => true,
            OnOff::Off => false,
        }
    }
}

impl Serialize for OnOff {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_bool(self.into())
    }
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct HolidayMode {
    onoff: OnOff,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum Meter {
    Flow {
        id: u16,
        name: String,
        flow_rate: f32,
    },
    Power {
        id: u16,
        name: String,
        power: f32,
    },
    Temperature {
        id: u16,
        temperature: f32,
    },
}

impl From<Meter> for f32 {
    fn from(value: Meter) -> Self {
        match value {
            Meter::Flow { flow_rate, .. } => flow_rate,
            Meter::Power { power, .. } => power,
            Meter::Temperature { temperature, .. } => temperature,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ValveData {
    onoff: OnOff,
    power: f32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RoomData {
    #[serde(rename = "deviceOnOff")]
    device_on_off: OnOff,
    power: f32,
    setpoint: f32,
    temperature: f32,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum Node {
    Room { id: u16, data: RoomData },
    Valve { id: u16, data: ValveData },
}

#[derive(Deserialize, Debug)]
struct Body {
    #[allow(dead_code)]
    holiday_mode: HolidayMode,
    #[allow(dead_code)]
    modbus_on_error: String,
    meters: Vec<Meter>,
    nodes: Vec<Node>,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct Headers {
    from: String,
    method: String,
    size: usize,
    #[serde(with = "date_format")]
    timestamp: DateTime<Utc>,
    to: String,
    #[serde(rename = "type")]
    _type: String,
    version: String,
}

#[derive(Deserialize, Debug)]
pub struct ESmartMessage {
    #[allow(dead_code)]
    headers: Headers,
    body: Body,
}

impl ESmartMessage {
    pub fn iter_meters(&self) -> std::slice::Iter<Meter> {
        self.body.meters.iter()
    }

    pub fn iter_nodes(&self) -> std::slice::Iter<Node> {
        self.body.nodes.iter()
    }
}

impl IterStats<std::vec::IntoIter<(String, Stat, f32)>> for &Meter {
    fn into_stats_iter(self) -> std::vec::IntoIter<(String, Stat, f32)> {
        match self {
            Meter::Flow {
                id,
                name,
                flow_rate,
            } => vec![(
                format!("flow_{id}"),
                Stat::new(
                    name,
                    Unit::Lmin,
                    DeviceClass::Water,
                    "mdi:pipe",
                    "flow_rate",
                ),
                *flow_rate,
            )],
            Meter::Power { id, name, power } => {
                vec![(
                    format!("power_{id}"),
                    Stat::new(
                        name,
                        Unit::W,
                        DeviceClass::Power,
                        "mdi:lightning-bolt",
                        "power",
                    ),
                    *power,
                )]
            }
            Meter::Temperature { id, temperature } => vec![(
                format!("temperature_{id}"),
                Stat::new(
                    "General Temperature",
                    Unit::C,
                    DeviceClass::Temperature,
                    "mdi:thermometer",
                    "temperature",
                ),
                *temperature,
            )],
        }
        .into_iter()
    }
}

impl IterStats<std::vec::IntoIter<(String, Stat, f32)>> for &Node {
    fn into_stats_iter(self) -> std::vec::IntoIter<(String, Stat, f32)> {
        match self {
            Node::Room {
                id,
                data:
                    RoomData {
                        temperature,
                        device_on_off,
                        power,
                        ..
                    },
            } => vec![
                (
                    format!("room_temperature_{id}"),
                    Stat::new(
                        &format!("Room {id} Temperature"),
                        Unit::C,
                        DeviceClass::Temperature,
                        "mdi:thermometer",
                        "temperature",
                    ),
                    *temperature,
                ),
                (
                    format!("room_heating_on_{id}"),
                    Stat::new(
                        &format!("Room {id} Heating On"),
                        Unit::None,
                        DeviceClass::None,
                        "mdi:switch",
                        "deviceOnOff",
                    ),
                    if device_on_off.into() { 1.0 } else { 0.0 },
                ),
                (
                    format!("room_heating_power_{id}"),
                    Stat::new(
                        &format!("Room {id} Heating Power"),
                        Unit::None,
                        DeviceClass::Temperature,
                        "mdi:thermometer",
                        "power",
                    ),
                    *power,
                ),
            ]
            .into_iter(),
            Node::Valve {
                id,
                data: ValveData { power, onoff },
            } => vec![
                (
                    format!("valve_power_{id}"),
                    Stat::new(
                        &format!("Valve {id} Power"),
                        Unit::None,
                        DeviceClass::None,
                        "mdi:valve",
                        "power",
                    ),
                    *power,
                ),
                (
                    format!("valve_on_{id}"),
                    Stat::new(
                        &format!("Valve {id} On"),
                        Unit::None,
                        DeviceClass::None,
                        "mdi:valve",
                        "onoff",
                    ),
                    if onoff.into() { 1.0 } else { 0.0 },
                ),
            ]
            .into_iter(),
        }
    }
}
