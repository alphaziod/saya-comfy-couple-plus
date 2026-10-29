use serde::{Deserialize, Serialize};

pub const API_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(try_from = "String", into = "String")]
pub struct Seed(String);

impl Seed {
    pub fn parse(value: &str) -> Result<Self, String> {
        let parsed = value
            .parse::<u64>()
            .map_err(|_| "Seed must be a decimal u64")?;
        if parsed.to_string() != value {
            return Err("Seed must use canonical decimal notation".into());
        }
        Ok(Self(value.into()))
    }
}

impl TryFrom<String> for Seed {
    type Error = String;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<Seed> for String {
    fn from(value: Seed) -> Self {
        value.0
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    pub characters: u32,
    pub assets: u32,
    pub jobs: u32,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    pub api_version: u32,
    pub app_version: &'static str,
    pub schema_version: u32,
    pub counts: Counts,
    pub comfy_connected: bool,
    pub implemented: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiError {
    pub code: &'static str,
    pub message: String,
    pub retryable: bool,
}
