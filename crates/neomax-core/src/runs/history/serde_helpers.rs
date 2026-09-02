use std::fmt;

use serde::de::{self, Deserializer, Visitor};

pub(crate) fn deserialize_optional_u32<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_option(OptionalU32Visitor)
}

struct OptionalU32Visitor;

impl<'de> Visitor<'de> for OptionalU32Visitor {
    type Value = Option<u32>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(
            "null, a non-negative integer, a decimal u32 string, or the orchestrator marker",
        )
    }

    fn visit_none<E>(self) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(None)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(None)
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(OptionalU32Visitor)
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        u32::try_from(value)
            .map(Some)
            .map_err(|_| E::custom("account number is outside the u32 range"))
    }

    fn visit_u128<E>(self, value: u128) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        u32::try_from(value)
            .map(Some)
            .map_err(|_| E::custom("account number is outside the u32 range"))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        u32::try_from(value)
            .map(Some)
            .map_err(|_| E::custom("account number must be a non-negative u32"))
    }

    fn visit_i128<E>(self, value: i128) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        u32::try_from(value)
            .map(Some)
            .map_err(|_| E::custom("account number must be a non-negative u32"))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        if !value.is_finite()
            || value.is_sign_negative()
            || value.fract() != 0.0
            || value > f64::from(u32::MAX)
        {
            return Err(E::custom(
                "account number must be a non-negative integer-valued number",
            ));
        }
        Ok(Some(value as u32))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        if value.is_empty() {
            return Err(E::custom("account number string cannot be empty"));
        }
        if value.eq_ignore_ascii_case("orch") {
            return Ok(None);
        }
        value
            .parse::<u32>()
            .map(Some)
            .map_err(|_| E::custom("account number string must contain a u32"))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.visit_str(&value)
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq)]
    struct Shape {
        #[serde(deserialize_with = "super::deserialize_optional_u32")]
        value: Option<u32>,
    }

    #[test]
    fn accepts_numeric_and_decimal_string_forms() {
        assert_eq!(
            serde_json::from_str::<Shape>(r#"{"value":2}"#)
                .unwrap()
                .value,
            Some(2)
        );
        assert_eq!(
            serde_json::from_str::<Shape>(r#"{"value":2.0}"#)
                .unwrap()
                .value,
            Some(2)
        );
        assert_eq!(
            serde_json::from_str::<Shape>(r#"{"value":"02"}"#)
                .unwrap()
                .value,
            Some(2)
        );
        assert_eq!(
            serde_json::from_str::<Shape>(r#"{"value":null}"#)
                .unwrap()
                .value,
            None
        );
        assert_eq!(
            serde_json::from_str::<Shape>(r#"{"value":"orch"}"#)
                .unwrap()
                .value,
            None
        );
    }

    #[test]
    fn rejects_negative_fractional_invalid_and_out_of_range_forms() {
        for value in [
            r#"{"value":-1}"#,
            r#"{"value":-0.5}"#,
            r#"{"value":2.5}"#,
            r#"{"value":""}"#,
            r#"{"value":"nope"}"#,
            r#"{"value":4294967296}"#,
            r#"{"value":true}"#,
        ] {
            assert!(
                serde_json::from_str::<Shape>(value).is_err(),
                "accepted {value}"
            );
        }
    }
}
