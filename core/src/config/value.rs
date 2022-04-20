use std::collections::{HashMap};
use std::ffi::OsStr;
use std::fmt;
use std::cmp::PartialEq;

use maplit::hashmap;
use regex::{Regex};
use serde::{de, Deserialize, Serialize, Deserializer};

type AnyValue = toml::value::Value;
type ValueResolver = fn(&str) -> &str;

const SECRET_NAME_REGEX: &str = r"^[[:alpha:]_][[:alpha:]_0-9]*$";
pub fn is_secret_name(s: &str) -> bool {
    let re = Regex::new(SECRET_NAME_REGEX).unwrap();
    re.is_match(s)
}
fn secret_resolver(s: &str) -> &str {
    match super::secret::var(s) {
        Some(ref r) => r,
        None => &format!("${{{}}}", s),
    }
}
fn detect_value_ref(s: &str) -> (&str, Option<ValueResolver>) {
    let re = Regex::new(r"\$\{([^\}]+)\}").unwrap();
    match re.captures(s) {
        Some(captures) => {
            let key = captures.get(1).unwrap().as_str();
            if is_secret_name(key) {
                (key, Some(secret_resolver))
            } else {
                panic!("invalid secret name: {} should match {}", key, SECRET_NAME_REGEX)
            }
        }
        None => (s, None)
    }
}
const RESOLVER_NAME_MAP: HashMap<usize, &'static str> = hashmap! {
    // function pointer to usize value
    secret_resolver as usize => "secret",
};

#[derive(Serialize)]
pub struct Value {
    pub value: String,
    #[serde(skip)]
    pub resolver: Option<ValueResolver>,
}
impl Value {
    pub fn new(value: &str) -> Self {
        Self {
            value: value.to_string(),
            resolver: None,
        }
    }
}
struct Visitor;
impl<'de> de::Visitor<'de> for Visitor {
    type Value = Value;
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "a string")
    }
    fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        let (value, resolver) = detect_value_ref(s);
        Ok(Value {value: value.to_string(), resolver})
    }
}
impl<'de> Deserialize<'de> for Value {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: Deserializer<'de> {
        deserializer.deserialize_str(Visitor{})
    }
}
impl AsRef<str> for Value {
    fn as_ref(&self) -> &str {
        if self.resolver.is_some() {
            self.resolver.unwrap()(&self.value)
        } else {
            self.value.as_ref()
        }
    }
}
impl AsRef<OsStr> for Value {
    fn as_ref(&self) -> &OsStr {
        let s: &str = self.as_ref();
        OsStr::new(s)
    }
}
impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.resolver.is_some() {
            write!(f, "<{}:{}>", 
                RESOLVER_NAME_MAP.get(&(self.resolver.unwrap() as usize)).unwrap(), 
                self.value
            )
        } else {
            write!(f, "{}", self.value)
        }
    }
}
impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.resolver.is_some() {
            f.debug_struct("Value")
                .field("value", &format!("<{}:{}>", 
                    RESOLVER_NAME_MAP.get(&(self.resolver.unwrap() as usize)).unwrap(), 
                    self.value))
                .finish()
        } else {
            f.debug_struct("Value")
                .field("value", &format!("{}", self.value))
                .finish()
        }
    }
}
impl PartialEq<String> for Value {
    fn eq(&self, other: &String) -> bool {
        let s: &str = self.as_ref();
        s == other.as_str()
    }
}
impl PartialEq<String> for &Value {
    fn eq(&self, other: &String) -> bool {
        let s: &str = self.as_ref();
        s == other.as_str()
    }
}


#[derive(Serialize)]
pub struct Any {
    pub value: AnyValue,
    #[serde(skip)]
    pub resolver: Option<ValueResolver>,
}
impl<'de> Deserialize<'de> for Any {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: Deserializer<'de> {
        let any = AnyValue::deserialize(deserializer)?;
        return match any {
            AnyValue::String(ref v) => {
                let (value, resolver) = detect_value_ref(v);
                Ok(Any {value: AnyValue::String(value.to_string()), resolver})
            },
            _ => Ok(Any{value: any, resolver: None})
        }
    }
}
impl AsRef<str> for Any {
    fn as_ref(&self) -> &str {
        if self.resolver.is_some() {
            // should always be string (see initialization code above)
            self.resolver.unwrap()(&self.value.as_str().unwrap())
        } else {
            match self.value {
                AnyValue::String(ref s) => s,
                AnyValue::Integer(i) =>&i.to_string(),
                AnyValue::Float(f) => &f.to_string(),
                AnyValue::Boolean(b) => if b { "true" } else { "false" },
                AnyValue::Datetime(ref s) => &s.to_string(),
                _ => serde_json::to_string(&self.value).unwrap().as_ref()
            }
        }
    }
}
impl AsRef<OsStr> for Any {
    fn as_ref(&self) -> &OsStr {
        let s: &str = self.as_ref();
        OsStr::new(s)
    }
}
impl fmt::Display for Any {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.resolver.is_some() {
            write!(f, "<{}:{}>", 
                RESOLVER_NAME_MAP.get(&(self.resolver.unwrap() as usize)).unwrap(), 
                self.value.as_str().unwrap()
            )
        } else {
            write!(f, "{}", self.value)
        }
    }
}
impl fmt::Debug for Any {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.resolver.is_some() {
            f.debug_struct("Any")
                .field("type", &self.value.type_str().to_string())
                .field("value", &format!("<{}:{}>", 
                    RESOLVER_NAME_MAP.get(&(self.resolver.unwrap() as usize)).unwrap(), 
                    self.value))
                .finish()
        } else {
            f.debug_struct("Any")
                .field("type", &self.value.type_str().to_string())
                .field("value", &format!("{}", self.value))
                .finish()
        }
    }
}

pub struct Sensitive {
    pub value: String,
}

impl Sensitive {
    pub fn new(value: &str) -> Self {
        Self {
            value: value.to_string()
        }
    }
}

impl AsRef<str> for Sensitive {
    fn as_ref(&self) -> &str {
        self.value.as_str()
    }
}
impl AsRef<OsStr> for Sensitive {
    fn as_ref(&self) -> &OsStr {
        let s: &str = self.as_ref();
        OsStr::new(s)
    }
}
impl fmt::Display for Sensitive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<sensitive>")
    }
}
impl fmt::Debug for Sensitive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Sensitive")
            .field("value", &"<sensitive>".to_string())
            .finish()
    }
}
