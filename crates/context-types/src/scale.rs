use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeScale {
    Universe,
    Galaxy,
    System,
    Planet,
    Moon,
}

impl ScopeScale {
    pub const fn rank(self) -> u8 {
        match self {
            Self::Universe => 0,
            Self::Galaxy => 1,
            Self::System => 2,
            Self::Planet => 3,
            Self::Moon => 4,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    Registry,
    Product,
    Repository,
    Program,
    Subsystem,
    Module,
    Package,
    Service,
    Component,
    File,
    Type,
    Function,
    Test,
    Resource,
}
