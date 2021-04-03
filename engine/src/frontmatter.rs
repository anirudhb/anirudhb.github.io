use chrono::NaiveDate;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Front matter that can be parsed at the beginning of a Markdown file.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Frontmatter {
    /// Title
    pub title: String,
    /// Date (optional)
    #[serde(
        serialize_with = "serialize_date",
        deserialize_with = "deserialize_date"
    )]
    pub date: Option<NaiveDate>,
    /// Estimated time to read (optional)
    pub time_to_read: Option<String>,
}

pub const DATE_FORMAT: &'static str = "%m/%d/%Y";

fn serialize_date<S: Serializer>(date: &Option<NaiveDate>, ser: S) -> Result<S::Ok, S::Error> {
    if let Some(date) = date {
        ser.serialize_some(&date.format(DATE_FORMAT).to_string())
    } else {
        ser.serialize_none()
    }
}

fn deserialize_date<'de, D: Deserializer<'de>>(der: D) -> Result<Option<NaiveDate>, D::Error> {
    use serde::de::{Error, Visitor};
    struct NaiveDateOptionVisitor;
    struct NaiveDateVisitor;

    impl<'de> Visitor<'de> for NaiveDateVisitor {
        type Value = NaiveDate;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a string in MM/DD/YYYY format")
        }

        fn visit_str<E: Error>(self, v: &str) -> Result<Self::Value, E> {
            NaiveDate::parse_from_str(v, DATE_FORMAT)
                .map_err(|e| Error::custom(format!("failed to parse: {}", e)))
        }
    }

    impl<'de> Visitor<'de> for NaiveDateOptionVisitor {
        type Value = Option<NaiveDate>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("an optional string in MM/DD/YYYY format")
        }

        fn visit_some<D: Deserializer<'de>>(self, der: D) -> Result<Self::Value, D::Error> {
            Ok(Some(der.deserialize_str(NaiveDateVisitor)?))
        }

        fn visit_none<E: Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
    }

    der.deserialize_option(NaiveDateOptionVisitor)
}

impl Frontmatter {
    pub fn parse_from_str(s: &str) -> serde_yaml::Result<Self> {
        serde_yaml::from_str(s)
    }
}
