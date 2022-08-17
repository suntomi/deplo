use std::error::Error;
use std::fs;

use crate::util::escalate;

pub enum Source<'a> {
    File(&'a str),
    Memory(&'a str),
}
impl<'a> Source<'a> {
    fn to_string(&self) -> String {
        match self {
            Self::File(path) => match fs::read_to_string(path) {
                Ok(v) => v,
                Err(e) => panic!("cannot read config at {}, err: {:?}", path, e)
            },
            Self::Memory(v) => v.to_string(),
        }
    }
    pub fn load_as<'de, C>(&self) -> Result<C, Box<dyn Error>> 
    where C: serde::de::DeserializeOwned {
        match toml::from_str::<C>(&self.to_string()) {
            Ok(c) => Ok(c),
            Err(err) => escalate!(Box::new(err))
        }
    }
}