use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavePage {
    pub expected_version: i64,
    pub title: String,
    pub markdown: String,
    pub properties: Value,
}
