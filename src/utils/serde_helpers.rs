// Serde helper modules for custom serialization/deserialization
//
// This module provides shared serialization utilities used across the crate.

use chrono::Duration;
use serde::{Deserialize, Deserializer, Serializer};

/// Serialize a Duration as seconds (i64)
pub fn serialize_duration<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_i64(duration.num_seconds())
}

/// Deserialize a Duration from seconds (i64)
pub fn deserialize_duration<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    let seconds = i64::deserialize(deserializer)?;
    Ok(Duration::seconds(seconds))
}

/// Module for serializing Duration with serde
/// Use with #[serde(with = "crate::utils::duration_serde")]
pub mod duration_serde {
    use chrono::Duration;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_i64(duration.num_seconds())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let seconds = i64::deserialize(deserializer)?;
        Ok(Duration::seconds(seconds))
    }
}

/// Module for serializing Optional Duration with serde
/// Use with #[serde(with = "crate::utils::optional_duration_serde")]
pub mod optional_duration_serde {
    use chrono::Duration;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(duration: &Option<Duration>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match duration {
            Some(d) => serializer.serialize_some(&d.num_seconds()),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let seconds_opt = Option::<i64>::deserialize(deserializer)?;
        Ok(seconds_opt.map(Duration::seconds))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct TestStruct {
        #[serde(with = "duration_serde")]
        duration: Duration,
        #[serde(with = "optional_duration_serde")]
        optional_duration: Option<Duration>,
    }

    #[test]
    fn test_duration_serialization() {
        let test = TestStruct {
            duration: Duration::seconds(3600),
            optional_duration: Some(Duration::minutes(30)),
        };

        let json = serde_json::to_string(&test).unwrap();
        let deserialized: TestStruct = serde_json::from_str(&json).unwrap();

        assert_eq!(test, deserialized);
    }

    #[test]
    fn test_optional_duration_none() {
        let test = TestStruct {
            duration: Duration::seconds(60),
            optional_duration: None,
        };

        let json = serde_json::to_string(&test).unwrap();
        let deserialized: TestStruct = serde_json::from_str(&json).unwrap();

        assert_eq!(test, deserialized);
    }
}
