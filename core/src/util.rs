// defer
pub struct ScopedCall<F: FnOnce()> {
    pub c: Option<F>
}
impl<F: FnOnce()> Drop for ScopedCall<F> {
    fn drop(&mut self) {
        self.c.take().expect("ScopedCall have not initialized")()
    }
}

#[macro_export]
macro_rules! macro_defer_expr { ($e: expr) => { $e } } // tt hack
#[macro_export]
macro_rules! macro_defer {
    ($($data: tt)*) => (
        let _scope_call = crate::util::ScopedCall {
            c: Some(|| -> () { $crate::macro_defer_expr!({ $($data)* }) })
        };
    )
}

pub use macro_defer as defer;

// func
#[macro_export]
macro_rules! macro_func {
    () => {{
        fn f() {}
        fn type_name_of<T>(_: T) -> &'static str {
            std::any::type_name::<T>()
        }
        let name = type_name_of(f);
        &name[..name.len() - 3]
    }}
}
pub use macro_func as func;

// erase
#[macro_export]
macro_rules! macro_void {
    ($($values: expr),*) => {()}
}
pub use macro_void as void;

// error
use std::fmt;
use std::error::Error;

#[derive(Debug)]
pub struct EscalateError {
    pub at: (&'static str, &'static str, u32),
    pub source: Box<dyn Error + 'static>
}
impl fmt::Display for EscalateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.source() {
            Some(err) => write!(f, "{} {}:{}\n{}", self.at.0, self.at.1, self.at.2, err),
            None => write!(f, "{} {}:{}\n", self.at.0, self.at.1, self.at.2),
        }
    }
}
impl Error for EscalateError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.source.as_ref())
    }
}

#[macro_export]
macro_rules! macro_make_escalation {
    ( $err:expr ) => {
        Box::new(crate::util::EscalateError {
            at: (crate::util::func!(), file!(), line!()),
            source: $err
        })
    }
}
pub use macro_make_escalation as make_escalation;

#[macro_export]
macro_rules! macro_escalate {
    ( $err:expr ) => {
        Err(Box::new(crate::util::EscalateError {
            at: (crate::util::func!(), file!(), line!()),
            source: $err
        }))
    }
}
pub use macro_escalate as escalate;

// envsubst
use regex::{Regex,Captures};
use std::collections::HashMap;

pub fn envsubst(src: &str) -> String {
    // for (k, v) in std::env::vars() {
    //     println!("envsubst:{} => {}({})", k, v, v.len());
    // }
    let envs: HashMap<String, String> = std::env::vars().collect();
    let re = Regex::new(r"\$\{([^\}]+)\}").unwrap();
    let content = re.replace_all(src, |caps: &Captures| {
        match envs.get(&caps[1]) {
            Some(s) => s.replace("\n", r"\n").replace(r"\", r"\\").replace(r#"""#, r#"\""#),
            None => return format!("${{{}}}", &caps[1])
        }
    });
    return content.to_string()
}

// serde
use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum UnitOrListOf<T> {
    Unit(T),
    List(Vec<T>)
}
impl<'a, T> IntoIterator for &'a UnitOrListOf<T> {
    type Item = &'a T;
    type IntoIter = std::vec::IntoIter<Self::Item>;
    fn into_iter(self) -> Self::IntoIter {
        match self {
            UnitOrListOf::Unit(v) => vec![v].into_iter(),
            UnitOrListOf::List(v) => v.iter().map(|v| v).collect::<Vec<Self::Item>>().into_iter()
        }
    }
}

// env
pub mod env {
    use std::env;
    pub fn var_or_die(key: &str) -> String {
        match env::var(key) {
            Ok(v) => v,
            Err(e) => {
                panic!("env {} not found: {}", key, e);
            }
        }
    }
}

// seal
use rand;
use base64;
use blake2::{VarBlake2b};
use blake2::digest::{Update, VariableOutput};
use sodalite::{
    box_ as box_up, box_keypair_seed, 
    BoxPublicKey, BoxSecretKey, BoxNonce,
    BOX_SECRET_KEY_LEN, BOX_PUBLIC_KEY_LEN, BOX_NONCE_LEN
};

#[derive(Debug)]
pub struct CryptoError {
    pub cause: String
}
impl fmt::Display for CryptoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.cause)
    }
}
impl Error for CryptoError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}
fn seal_nonce(epk: &impl AsRef<[u8]>, pk: &impl AsRef<[u8]>, nonce: &mut BoxNonce) {
    let mut hasher = VarBlake2b::new(BOX_NONCE_LEN).unwrap();
    hasher.update(epk);
    hasher.update(pk);
    hasher.finalize_variable(|bs| {
        for i in 0..BOX_NONCE_LEN { nonce[i] = bs[i]; }
    });
}
pub fn randombytes(x: &mut [u8]) {
    let mut rng = rand::rngs::OsRng;
    use rand::RngCore;
    rng.fill_bytes(x);
}
#[macro_export]
macro_rules! macro_randombytes_as_string {
    ( $len:expr ) => {{
        let mut bytes = [0u8; $len];
        crate::util::randombytes(&mut bytes);
        base64::encode_config(&bytes, base64::URL_SAFE_NO_PAD)
    }}
}
pub use macro_randombytes_as_string as randombytes_as_string;
pub fn seal(plaintext: &str, pkey_encoded: &str) -> Result<String, Box<dyn Error>> {
    let pk_vec = base64::decode(pkey_encoded)?;
    if pk_vec.len() != 32 {
        return escalate!(Box::new(CryptoError {
            cause: format!("given encoded public key length wrong {}", pk_vec.len())
        }));
    }
    let mut pk: [u8; 32] = [0u8; 32];
    for i in 0..32 { pk[i] = pk_vec[i]; };
    // println!("plaintext:{}({})", plaintext, plaintext.len());

    let mut epk: BoxPublicKey = [0u8; BOX_PUBLIC_KEY_LEN];
    let mut esk: BoxSecretKey = [0u8; BOX_SECRET_KEY_LEN];
    let mut seed = [0u8; 32];
    randombytes(&mut seed);
    box_keypair_seed(&mut epk, &mut esk, &seed);

    let mut nonce: BoxNonce = [0u8; BOX_NONCE_LEN];
    seal_nonce(&epk, &pk, &mut nonce);
    let plaintext_as_bytes = plaintext.as_bytes();
    let mut padded_plaintext = vec![0u8; plaintext_as_bytes.len() + 32];
    for i in 0..32 { padded_plaintext[i] = 0; }
    for i in 0..plaintext_as_bytes.len() { padded_plaintext[i + 32] = plaintext_as_bytes[i]; }
    let mut ciphertext = vec![0; padded_plaintext.len()];

    box_up(&mut ciphertext, &padded_plaintext, &nonce, &pk, &esk).unwrap();

    // println!("padded_plaintext:{:?}", padded_plaintext);
    // println!("ciphertext:{:?}", ciphertext);

    let mut result_vec = vec!();
    result_vec.extend_from_slice(&epk);
    for i in 16..ciphertext.len() { result_vec.push(ciphertext[i]) }

    Ok(base64::encode(result_vec))
}

// ref
pub fn to_kv_ref<'a>(h: &'a HashMap<String, String>) -> HashMap<&'a str, &'a str> {
    let mut ret = HashMap::<&'a str, &'a str>::new();
    for (k, v) in h {
        ret.entry(k).or_insert(v);
    }
    return ret;
}

// multiline string which can specify indentation of each line
// from width format specifier. useful for print multiline element in yaml
pub trait Iterable<'a> {
    type Item: 'a;
    type Iter: Iterator<Item = Self::Item>;
    fn iterator(&'a self) -> Self::Iter;
}
impl<'a, T> Iterable<'a> for Vec<T> where T: 'a {
    type Item = &'a T;
    type Iter = std::slice::Iter<'a,T>;
    fn iterator(&'a self) -> Self::Iter {
        self.iter()
    }
}
pub struct MultilineFormatString<'a, I, V>
where I: Iterable<'a, Item = V>, V: AsRef<str> {
    pub strings: &'a I,
    pub postfix: Option<&'a str>
}
impl<'a, I, V> fmt::Display for MultilineFormatString<'a, I, V> 
where I: Iterable<'a, Item = V>, V: AsRef<str> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let indent = f.width().unwrap_or(0);
        let mut it = self.strings.iterator();
        // iterate over element and index
        let mut v = it.next();
        loop {
            match v {
                Some(s) => {
                    for _ in 0..indent {
                        write!(f, " ")?;
                    }
                    f.write_str(s.as_ref())?;
                    v = it.next();
                    if v.is_some() {
                        match self.postfix {
                            Some(v) => f.write_str(&format!("{}\n", v))?,
                            None => f.write_str("\n")?
                        }
                    }
                },
                None => break
            }
        }
        //TODO: if strings has no element, we cannot cancel linefeed in template.
        //need to find someway to do this.
        Ok(())
    }
}

// fs
use fs_extra;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path,PathBuf};
pub fn rm<P: AsRef<Path>>(path: P) -> bool {
    match fs::remove_file(path.as_ref()) {
        Ok(_) => return true,
        Err(err) => { 
            log::error!(
                "fail to remove {} with {:?}", path.as_ref().to_string_lossy(), err
            );
            return false
        },
    }
}
pub fn rmdir<P: AsRef<Path>>(path: P) -> bool {
    match fs::remove_dir_all(path.as_ref()) {
        Ok(_) => return true,
        Err(err) => { 
            log::error!(
                "fail to cleanup for reinitialize: fail to remove {} with {:?}",
                path.as_ref().to_string_lossy(), err
            );
            return false
        },
    }
}
pub fn dircp<P: AsRef<Path>>(src: P, dest: P) -> Result<(), Box<dyn Error>> {
    match fs::metadata(dest.as_ref()) {
        Ok(_) => log::debug!(
            "{}({:?}) already copied",
            dest.as_ref().to_string_lossy(), fs::canonicalize(&dest)
        ),
        Err(_) => {
            log::debug!("copy infra setup scripts: {}=>{}", 
                src.as_ref().to_string_lossy(), dest.as_ref().to_string_lossy());
            fs_extra::dir::copy(
                src, dest,
                &fs_extra::dir::CopyOptions{
                    content_only: true,
                    overwrite: true,
                    skip_exist: false,
                    buffer_size: 64 * 1024, //64kb
                    copy_inside: true,
                    depth: 0
                }
            )?;
        }
    }
    Ok(())
}
pub fn write_file<F, P: AsRef<Path>>(dest: P, make_contents: F) -> Result<bool, Box<dyn Error>> 
where F: Fn () -> Result<String, Box<dyn Error>> {
    match fs::metadata(dest.as_ref()) {
        Ok(_) => {
            log::debug!(
                "file {}({:?}) already exists",
                dest.as_ref().to_string_lossy(), fs::canonicalize(&dest)
            );
            Ok(false)
        },
        Err(_) => {
            log::debug!("create file {}", dest.as_ref().to_string_lossy());
            fs::write(&dest, &make_contents()?)?;
            Ok(true)
        }
    }
}
pub fn make_absolute(rel_or_abs: impl AsRef<OsStr>, root_directory: impl AsRef<OsStr>) -> PathBuf {
    {
        let path = Path::new(&rel_or_abs);
        if path.is_absolute() {
            return rel_or_abs.as_ref().to_owned().into();
        }
    }
    return path_join(vec![root_directory.as_ref(), rel_or_abs.as_ref()]);
}

pub fn path_join(components: Vec<impl AsRef<OsStr>>) -> PathBuf {
    // if os is windows and MSYSTEM is set, use / for path separator,
    // otherwise use rust's std::path::Path
    let skewed_sep = if let Ok(msystem) = std::env::var("MSYSTEM") {
        // nest this match inside if-let to more easily unwrap and call .as_str
        match msystem.as_str() {
            "MINGW64" | "MINGW32" | "MSYS" => Some("/".to_owned()),
            _ => None,
        }
    } else {
        None
    };
    match skewed_sep {
        Some(ref v) => {
            PathBuf::from(components.iter().map(|c| c.as_ref().to_str().unwrap()).collect::<Vec<&str>>().join(v))
        },
        None => {
            let mut buf = PathBuf::new();
            for c in components {
                buf.push(c.as_ref().to_str().unwrap());
            }
            buf
        }
    }
}
pub fn docker_mount_path(path: &str) -> String {
    if let Ok(msystem) = std::env::var("MSYSTEM") {
        match msystem.as_str() {
            "MINGW64" | "MINGW32" | "MSYS" => {},
            _ => return path.to_string(),
        }
    } else {
        return path.to_string();
    };
    if path == "/var/run/docker.sock" {
        return "//var/run/docker.sock".to_string();
    }
    let re = regex::Regex::new(r"(^[A-Z]):(.*)").unwrap();
    match re.captures(&path) {
        Some(c) => {
            return format!("//{}{}", c.get(1).unwrap().as_str().to_lowercase(), c.get(2).unwrap().as_str());
        },
        None => {}
    };
    let re = regex::Regex::new(r"^/([a-z])/(.*)").unwrap();
    match re.captures(&path) {
        Some(c) => {
            return format!("//{}/{}", c.get(1).unwrap().as_str(), c.get(2).unwrap().as_str());
        },
        None => {}
    };
    return path.to_string();
}
pub fn find_repository_root() -> Result<PathBuf, Box<dyn Error>> {
    let mut current_dir = std::env::current_dir().unwrap();
    loop {
        if current_dir.join(".git").exists() {
            log::debug!("Found git repository at {}", current_dir.display());
            return Ok(current_dir);
        }
        if !current_dir.pop() {
            return Err("No git repository found".into());
        }
    }
}

// hashmap utils
use crc::{Crc, CRC_64_ECMA_182};

const CRC_64: Crc<u64> = Crc::<u64>::new(&CRC_64_ECMA_182);

pub fn maphash(h: &HashMap<String, crate::config::Value>) -> String {
    // hash value for separating repository cache according to checkout options
    let mut v: Vec<_> = h.into_iter().collect();
    let mut digest = CRC_64.digest();
    v.sort_by(|x,y| x.0.cmp(&y.0));
    digest.update(v.iter().map(|(k,v)| format!("{}={}", k, v)).collect::<Vec<String>>().join(",").as_bytes());
    format!("{:X}", digest.finalize())
}

pub fn sorted_key_iter<K: std::cmp::Ord,V>(h: &HashMap<K, V>) -> impl Iterator<Item=(&K, &V)> {
    let mut v: Vec<_> = h.into_iter().collect();
    v.sort_by(|x,y| x.0.cmp(&y.0));
    v.into_iter()
}

pub fn merge_hashmap<K: std::cmp::Eq + std::hash::Hash + Clone, V: Clone>(h1: &HashMap<K, V>, h2: &HashMap<K, V>) -> HashMap<K, V> {
    let mut ret = HashMap::new();
    for (k, v) in h1.into_iter() {
        ret.insert(k.clone(), v.clone());
    }
    for (k, v) in h2.into_iter() {
        ret.insert(k.clone(), v.clone());
    }
    return ret;
}

// strhash
pub fn strhash(src: &str) -> String {
    // hash value for separating repository cache according to checkout options
    let mut digest = CRC_64.digest();
    digest.update(src.as_bytes());
    format!("{:X}", digest.finalize())
}

// escape
pub fn escape(input: &str) -> String {
    let mut escaped = String::new();
    for c in input.chars() {
        match c {
            '\x00' => escaped.push_str("\\0"),
            '\x07' => escaped.push_str("\\a"),
            '\x08' => escaped.push_str("\\b"),
            ' ' => escaped.push_str("\\ "),
            '\t' => escaped.push_str("\\t"),
            '\n' => escaped.push_str("\\n"),
            '\x0b' => escaped.push_str("\\v"),
            '\x0c' => escaped.push_str("\\f"),
            '\r' => escaped.push_str("\\r"),
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\'' => escaped.push_str("\\'"),
            _ => escaped.push(c),
        }
    }    
    escaped
}   

// vec
pub fn join_vector<T>(src: Vec<Vec<T>>) -> Vec<T> {
    let mut r = vec![];
    for v in src {
        for e in v {
            r.push(e)
        }
    }
    r
}

// debugger
#[macro_export]
macro_rules! macro_try_debug {
    ($ctx: expr, $ci: expr, $config: expr, $failure: expr) => {
        if $config.debug_should_start($ctx, $failure) {
            log::info!("start debugger for job {} by failure {}, config {}:{:?}",
                $ctx, $failure, $config.debug, $config.debug_job);
        } else {
            $ci.set_job_env(hashmap!{
                "DEPLO_CI_RUN_DEBUGGER" => ""
            })?;
        }        
    }
}
pub use macro_try_debug as try_debug;


// json
// same as serde_json::from_str, but support the case that s represents single number/boolean/null.
// serde_json::from_str does not seem to support them.
pub fn str_to_json(s: &str) -> serde_json::Value {
    match serde_json::from_str(s) {
        Ok(v) => v,
        Err(_) => {
            match serde_json::from_str::<serde_json::Value>(&format!("{{\"v\":\"{}\"}}", s)) {
                // if s is null/true/false/number, from_str should be success.
                Ok(v) => v.as_object().unwrap().get("v").unwrap().clone(),
                Err(_) => {
                    // otherwise it should be string
                    serde_json::Value::String(s.to_string())
                }
            }
        }
    }    
}
// #[macro_export]
// macro_rules! macro_jsonpath {
//     ( $src:expr, $path:expr ) => {
//         jsonpath_lib::select_as_str($src, $path)
//     }
// }
// pub use macro_jsonpath as jsonpath;

pub fn json_to_strmap(v: &serde_json::Value) -> HashMap<&str, String> {
    let mut ret = HashMap::new();
    if let Some(obj) = v.as_object() {
        for (k, v) in obj {
            if let Some(s) = v.as_str() {
                ret.insert(k.as_str(), s.to_string());
            } else {
                ret.insert(k.as_str(), v.to_string());
            }
        }
    }
    ret
}

pub fn jsonpath(src: &str, expr: &str) -> Result<Option<String>, Box<dyn Error>> {
    let filtered = jsonpath_lib::select_as_str(src, expr)?;
    let json = str_to_json(&filtered);
    if json.is_array() {
        let len = json.as_array().unwrap().len();
        if len == 0 {
            return Ok(None);
        } else if len == 1 {
            let obj = &json.as_array().unwrap()[0];
            if obj.is_string() {
                return Ok(Some(obj.as_str().unwrap().to_string()));
            } else {
                return Ok(Some(obj.to_string()));
            }
        } else {
            return Ok(Some(filtered))
        }
    } else {
        Ok(None)
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use maplit::hashmap;

    #[test]
    fn seal_test() {
        let pk: [u8; 32] = [
            251, 131, 196, 215, 71, 235, 222, 20, 23, 114, 62, 99, 207, 12, 107, 139,
            240, 115, 104, 188, 0, 166, 113, 163, 146, 192, 226, 36, 237, 60, 205, 33
        ];
        let plaintext_as_bytes: [u8; 13] = [
            115, 117, 110, 116, 111, 109, 105, 44, 32, 105, 110, 99, 46
        ];
        let epk: BoxPublicKey = [
            116, 145, 95, 55, 158, 53, 28, 119, 63, 24, 168, 231, 86, 102,
            170, 53, 76, 165, 29, 123, 124, 245, 100, 251, 25, 164, 144, 220,
            13, 57, 238, 37
        ];
        let esk: BoxSecretKey = [
            151, 93, 67, 130, 106, 170, 176, 198, 242, 76, 136, 220, 39, 233,
            129, 103, 243, 197, 224, 222, 225, 56, 80, 22, 219, 173, 121, 12,
            213, 155, 44, 165
        ];
        let mut nonce: BoxNonce = [0u8; BOX_NONCE_LEN];
        seal_nonce(&epk, &pk, &mut nonce);
        println!("epk:{:?},pk:{:?},nonce:{:?}", epk, pk, nonce);
        let mut padded_plaintext = vec![0u8; plaintext_as_bytes.len() + 32];
        for i in 0..32 { padded_plaintext[i] = 0; }
        for i in 0..plaintext_as_bytes.len() { padded_plaintext[i + 32] = plaintext_as_bytes[i]; }
        let mut ciphertext = vec![0; padded_plaintext.len()];

        box_up(&mut ciphertext, &padded_plaintext, &nonce, &pk, &esk).unwrap();

        println!("padded_plaintext:{:?}", padded_plaintext);
        println!("ciphertext:{:?}", ciphertext);

        let mut result_vec = vec!();
        result_vec.extend_from_slice(&epk);
        for i in 16..ciphertext.len() { result_vec.push(ciphertext[i]) }

        let result = base64::encode(result_vec);
        let expect = "dJFfN541HHc/GKjnVmaqNUylHXt89WT7GaSQ3A057iW40mR0MOenhwM21mgQ3aL/kzlCvXvKLDBFzoiXAA==";
        println!("result:{},expect:{}", result, expect);
        assert!(result == expect);
    }

    #[test]
    fn str_to_json_test() {
        let s = r#"{"a":1,"b":2}"#;
        let v = str_to_json(s);
        // println!("{}", v);
        assert!(v.as_object().unwrap().get("a").unwrap().as_i64().unwrap() == 1);
        assert!(v.as_object().unwrap().get("b").unwrap().as_i64().unwrap() == 2);
        let s = r#"1"#;
        let v = str_to_json(s);
        assert!(v.as_i64().unwrap() == 1);
        let s = r#"true"#;
        let v = str_to_json(s);
        assert!(v.as_bool().unwrap() == true);
        let s = r#"null"#;
        let v = str_to_json(s);
        assert!(v.is_null());
    }

    #[test]
    fn json_path_test() {
        let json_obj = serde_json::json!({
            "store": {
                "book": [
                    {
                        "category": "reference",
                        "author": "Nigel Rees",
                        "title": "Sayings of the Century",
                        "price": 8.95
                    },
                    {
                        "category": "fiction",
                        "author": "Evelyn Waugh",
                        "title": "Sword of Honour",
                        "price": 12.99
                    },
                    {
                        "category": "fiction",
                        "author": "Herman Melville",
                        "title": "Moby Dick",
                        "isbn": "0-553-21311-3",
                        "price": 8.99
                    },
                    {
                        "category": "fiction",
                        "author": "J. R. R. Tolkien",
                        "title": "The Lord of the Rings",
                        "isbn": "0-395-19395-8",
                        "price": 22.99
                    }
                ],
                "bicycle": {
                    "color": "red",
                    "price": 19.95
                }
            },
            "expensive": 10
        });
        let s = serde_json::to_string(&json_obj).unwrap();
        assert_eq!(str_to_json(&jsonpath(&s, "$.store.book[*].author").unwrap().unwrap()),
            serde_json::json!([
                "Nigel Rees", "Evelyn Waugh", "Herman Melville", "J. R. R. Tolkien"
            ])
        );
        assert_eq!(jsonpath(&s, "$.store.book[*].neither").unwrap(),
            None
        );
        assert_eq!(str_to_json(&jsonpath(&s, "$.store.book[?(@.price < 10)]").unwrap().unwrap()),
            serde_json::json!([
                &serde_json::json!({"category" : "reference","author" : "Nigel Rees","title" : "Sayings of the Century","price" : 8.95}),
                &serde_json::json!({"category" : "fiction","author" : "Herman Melville","title" : "Moby Dick","isbn" : "0-553-21311-3","price" : 8.99})
            ])
        );
        assert_eq!(jsonpath(&s, "$.store.book[?(@.price > 100)]").unwrap(),
            None
        );
        
        let json_obj2 = serde_json::json!({
            "head": {
                "ref": "hoge",
                "title": "hogehoge",
                "number": 1,
                "boolean": false
            },
            "base": {
                "ref": "fuga",
                "title": "fugafuga",
                "obj": {
                    "key": "value"
                },
                "array": [1,2,3]
            },
        });
        let s2 = serde_json::to_string(&json_obj2).unwrap();
        assert_eq!(&jsonpath(&s2, "$.head.title").unwrap().unwrap(), "hogehoge");
        assert_eq!(&jsonpath(&s2, "$.head.number").unwrap().unwrap(), "1");
        assert_eq!(&jsonpath(&s2, "$.head.boolean").unwrap().unwrap(), "false");
        assert_eq!(&jsonpath(&s2, "$.base.ref").unwrap().unwrap(), "fuga");
        assert_eq!(&jsonpath(&s2, "$.base.obj").unwrap().unwrap(), r#"{"key":"value"}"#);
        assert_eq!(&jsonpath(&s2, "$.base.array").unwrap().unwrap(), "[1,2,3]");
    }

    #[test]
    fn docker_mount_path_test() {
        let testcase = hashmap!{
            "C:/foo/bar" => "//c/foo/bar",
            "E:/bar/baz" => "//e/bar/baz",
            "/usr/local/bin" => "/usr/local/bin",
            "/var/run/docker.sock" => "//var/run/docker.sock",
        };
        std::env::set_var("MSYSTEM", "MINGW64");
        for (input, expect) in testcase.iter() {
            assert_eq!(docker_mount_path(input), expect.to_string());
        }
    }
}
