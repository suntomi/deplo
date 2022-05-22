use std::fmt;
use std::cmp::PartialEq;
use std::borrow::Cow;

use regex::{Regex};
use serde::{de, Deserialize, Serialize, Deserializer};

use crate::config;

type AnyValue = toml::value::Value;
type ValueResolver = fn(&str) -> String;

const SECRET_NAME_REGEX: &str = r"^[[:alpha:]_][[:alpha:]_0-9]*$";
pub fn is_secret_name(s: &str) -> bool {
    let re = Regex::new(SECRET_NAME_REGEX).unwrap();
    re.is_match(s)
}
fn secret_resolver(s: &str) -> String {
    match config::secret::var(s) {
        Some(r) => r,
        None => format!("${{{}}}", s),
    }
}
fn detect_value_ref(s: &str) -> (&str, Option<ValueResolver>) {
    let re = Regex::new(r"^\$\{([^\}]+)\}$").unwrap();
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

fn resolver_to_name(resolver: ValueResolver) -> &'static str {
    let sz = resolver as usize;
    if sz == (secret_resolver as usize) { "secret" }
    else { panic!("unknown resolver {}", sz) }
}

#[derive(Serialize, Clone)]
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
    pub fn as_str(&self) -> &str {
        self.value.as_str()
    }
    pub fn resolve(&self) -> String {
        if self.resolver.is_some() {
            self.resolver.unwrap()(&self.value)
        } else {
            self.value.clone()
        }
    }
    pub fn to_arg<'a>(&self) -> crate::shell::Arg<'a> {
        Box::new(self.clone())
    }
    // for map operation
    pub fn resolve_to_string(value: &Self) -> String {
        value.resolve()
    }
}
impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        return self.value == other.value && 
            self.resolver.map_or_else(|| 0, |r| r as usize) == 
            other.resolver.map_or_else(|| 0, |r| r as usize)
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
impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.resolver.is_some() {
            write!(f, "<{}:{}>", resolver_to_name(self.resolver.unwrap()), self.value)
        } else {
            write!(f, "{}", self.value)
        }
    }
}
impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.resolver.is_some() {
            f.debug_struct("Value")
                .field("value", &format!("<{}:{}>", resolver_to_name(self.resolver.unwrap()), self.value))
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
        let s: &str = &self.resolve();
        s == other.as_str()
    }
}
impl PartialEq<String> for &Value {
    fn eq(&self, other: &String) -> bool {
        let s: &str = &self.resolve();
        s == other.as_str()
    }
}
impl crate::shell::ArgTrait for Value {
    fn value(&self) -> String {
        self.resolve()
    }
    fn view(&self) -> Cow<'_, str> {
        Cow::Owned(format!("{}", self))
    }
}


#[derive(Serialize, Clone)]
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
impl Any {
    pub fn new(value: &str) -> Self {
        Self {
            value: AnyValue::String(value.to_string()),
            resolver: None,
        }
    }
    pub fn resolve(&self) -> String {
        if self.resolver.is_some() {
            // should always be string (see initialization code above)
            self.resolver.unwrap()(&self.value.as_str().unwrap())
        } else {
            match self.value {
                AnyValue::String(ref s) => s.clone(),
                AnyValue::Integer(i) => i.to_string(),
                AnyValue::Float(f) => f.to_string(),
                AnyValue::Boolean(b) => (if b { "true" } else { "false" }).to_string(),
                AnyValue::Datetime(ref s) => s.to_string(),
                _ => serde_json::to_string(&self.value).unwrap()
            }
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self.value {
            AnyValue::String(ref s) => Some(s),
            _ => None
        }
    }
    pub fn at(&self, i: usize) -> Option<Any> {
        match &self.value {
            AnyValue::Array(a) => if a.len() > i {
                Some(Any{value: a[i].clone(), resolver: self.resolver})
            } else {
                None
            },
            _ => None
        }
    }
    pub fn index(&self, k: &str) -> Option<Any> {
        match &self.value {
            AnyValue::Table(t) => match t.get(k) {
                Some(v) => Some(Any{value: v.clone(), resolver: self.resolver}),
                None => None,
            }
            _ => None
        }
    }
}
impl fmt::Display for Any {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.resolver.is_some() {
            write!(f, "<{}:{}>", 
                resolver_to_name(self.resolver.unwrap()), 
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
                    resolver_to_name(self.resolver.unwrap()), 
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
    fn resolve(&self) -> String {
        self.value.clone()
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
impl crate::shell::ArgTrait for Sensitive {
    fn value(&self) -> String {
        self.resolve()
    }
    fn view(&self) -> Cow<'_, str> {
        Cow::Borrowed("<sensitive>")
    }
}
