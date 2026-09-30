use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RichTextStyle {
    Bold,
    Italic,
    Underline,
    Strike,
    Monospace,
    Quote,
    Heading,
    Link,
    Mention,
    Animoji,
}

impl RichTextStyle {
    pub fn wire_name(&self) -> &'static str {
        match self {
            Self::Bold => "STRONG",
            Self::Italic => "EMPHASIZED",
            Self::Underline => "UNDERLINE",
            Self::Strike => "STRIKETHROUGH",
            Self::Monospace => "MONOSPACED",
            Self::Quote => "QUOTE",
            Self::Heading => "HEADING",
            Self::Link => "LINK",
            Self::Mention => "USER_MENTION",
            Self::Animoji => "ANIMOJI",
        }
    }

    pub fn from_wire_name(name: &str) -> Option<Self> {
        match name {
            "STRONG" => Some(Self::Bold),
            "EMPHASIZED" => Some(Self::Italic),
            "UNDERLINE" => Some(Self::Underline),
            "STRIKETHROUGH" => Some(Self::Strike),
            "MONOSPACED" => Some(Self::Monospace),
            "QUOTE" => Some(Self::Quote),
            "HEADING" => Some(Self::Heading),
            "LINK" => Some(Self::Link),
            "USER_MENTION" => Some(Self::Mention),
            "ANIMOJI" => Some(Self::Animoji),
            _ => None,
        }
    }
}

impl Serialize for RichTextStyle {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.wire_name())
    }
}

impl<'de> Deserialize<'de> for RichTextStyle {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_wire_name(&s).ok_or_else(|| serde::de::Error::custom(format!("unknown wire style: {}", s)))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContentElement {
    #[serde(rename = "type")]
    pub style: RichTextStyle,
    pub from: usize,
    pub length: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<HashMap<String, serde_json::Value>>,
}

impl ContentElement {
    pub fn new(style: RichTextStyle, from: usize, length: usize) -> Self {
        Self {
            style,
            from,
            length,
            entity_id: None,
            entity_name: None,
            attributes: None,
        }
    }

    pub fn with_link(from: usize, length: usize, url: String) -> Self {
        let mut attrs = HashMap::new();
        attrs.insert("url".to_string(), serde_json::Value::String(url));
        Self {
            style: RichTextStyle::Link,
            from,
            length,
            entity_id: None,
            entity_name: None,
            attributes: Some(attrs),
        }
    }

    pub fn with_mention(from: usize, length: usize, user_id: i64, name: Option<String>) -> Self {
        Self {
            style: RichTextStyle::Mention,
            from,
            length,
            entity_id: Some(user_id),
            entity_name: name,
            attributes: None,
        }
    }
}
