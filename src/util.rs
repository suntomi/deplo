// defer
pub struct ScopedCall<F: FnOnce()> {
    pub c: Option<F>
}
impl<F: FnOnce()> Drop for ScopedCall<F> {
    fn drop(&mut self) {
        self.c.take().unwrap()()
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
macro_rules! macro_escalate  {
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
    let envs: HashMap<String, String> = std::env::vars().collect();
    for (k, v) in &envs {
        println!("envsubst:{}=>{}", k, v);
    };
    let re = Regex::new(r"\$\{([^\}]+)\}").unwrap();
    let content = re.replace_all(src, |caps: &Captures| {
        match envs.get(&caps[1]) {
            Some(s) => s.replace("\n", r"\n").replace(r"\", r"\\").replace(r#"""#, r#"\""#),
            None => return format!("${{{}}}", &caps[1])
        }
    });
    return content.to_string()
}

// seal
use rand;
use base64;
use crypto_box::{SalsaBox, PublicKey, SecretKey, aead::Aead};

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
pub fn seal(plaintext: &str, pkey_encoded: &str) -> Result<String, Box<dyn Error>> {
    let mut rng = rand::thread_rng();
    let secret_key = SecretKey::generate(&mut rng);
    let vec_pkey_decoded = base64::decode(pkey_encoded)?;
    if vec_pkey_decoded.len() != 32 {
        return escalate!(Box::new(CryptoError {
            cause: format!("given encoded public key length wrong {}", vec_pkey_decoded.len())
        }));
    }
    let mut pkey_decoded: [u8; 32] = [0; 32];
    for i in 0..31 { pkey_decoded[i] = vec_pkey_decoded[i]; };
    let public_key = PublicKey::from(pkey_decoded);
    let bx = SalsaBox::new(&public_key, &secret_key);
    let nonce = crypto_box::generate_nonce(&mut rng);
    let encrypted_bin = match bx.encrypt(&nonce, plaintext.as_bytes()) {
        Ok(vec) => vec,
        Err(e) => return escalate!(Box::new(CryptoError {
            cause: format!("encrypt failure {:?}", e)
        }))
    };
    let mut result_vec = vec!();
    result_vec.extend_from_slice(&secret_key.to_bytes());
    result_vec.extend(encrypted_bin);
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