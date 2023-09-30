#[derive(Clone)]
pub enum Unit {
    None,
    C,
    Lmin,
    W,
}

#[derive(Clone)]
pub enum DeviceClass {
    None,
    Water,
    Power,
    Temperature,
}

#[derive(Clone)]
pub struct Stat {
    name: String,
    unit: Unit,
    device_class: DeviceClass,
    icon: String,
    property: String,
}

impl Stat {
    pub fn new(
        name: &str,
        unit: Unit,
        device_class: DeviceClass,
        icon: &str,
        property: &str,
    ) -> Self {
        Self {
            name: name.into(),
            unit,
            device_class,
            icon: icon.into(),
            property: property.into(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn icon(&self) -> &str {
        &self.icon
    }

    pub fn property(&self) -> &str {
        &self.property
    }

    pub fn device_class_str(&self) -> &str {
        match self.device_class {
            DeviceClass::None => "",
            DeviceClass::Water => "water",
            DeviceClass::Power => "power",
            DeviceClass::Temperature => "temperature",
        }
    }

    pub fn unit_str(&self) -> &str {
        match self.unit {
            Unit::None => "",
            Unit::C => "C",
            Unit::Lmin => "L/min",
            Unit::W => "W",
        }
    }
}

pub trait IterStats<I: IntoIterator> {
    fn into_stats_iter(self) -> I;
}
