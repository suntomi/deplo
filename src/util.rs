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
    for (k, v) in std::env::vars() {
        println!("envsubst:{} => {}", k, v);
    }
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

// seal
use rand;
use base64;
use blake2::{Blake2b, Digest};
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
    let mut hasher = Blake2b::new();
    hasher.update(epk);
    hasher.update(pk);
    let bs = hasher.finalize();
    for i in 0..BOX_NONCE_LEN { nonce[i] = bs[i]; }
}
pub fn randombytes(x: &mut [u8]) {
    let mut rng = rand::rngs::OsRng;
    use rand::RngCore;
    rng.fill_bytes(x);
}
pub fn seal(plaintext: &str, pkey_encoded: &str) -> Result<String, Box<dyn Error>> {
    let pk_vec = base64::decode(pkey_encoded)?;
    if pk_vec.len() != 32 {
        return escalate!(Box::new(CryptoError {
            cause: format!("given encoded public key length wrong {}", pk_vec.len())
        }));
    }
    let mut pk: [u8; 32] = [0u8; 32];
    for i in 0..31 { pk[i] = pk_vec[i]; };

    let mut epk: BoxPublicKey = [0u8; BOX_PUBLIC_KEY_LEN];
    let mut esk: BoxSecretKey = [0u8; BOX_SECRET_KEY_LEN];
    let mut seed = [0u8; 32];
    randombytes(&mut seed);
    box_keypair_seed(&mut epk, &mut esk, &seed);

    let mut nonce: BoxNonce = [0u8; BOX_NONCE_LEN];
    seal_nonce(&epk, &pk, &mut nonce);
    let plaintext_as_bytes = plaintext.as_bytes();
    let mut padded_plaintext = vec![0u8; plaintext_as_bytes.len() + 32];
    for _ in 0..31 { padded_plaintext.push(0); }
    for i in 0..plaintext_as_bytes.len() { padded_plaintext.push(plaintext_as_bytes[i]); }
    let mut ciphertext = vec![0; padded_plaintext.len()];

    box_up(&mut ciphertext, &padded_plaintext, &nonce, &pk, &esk).unwrap();

    let mut result_vec = vec!();
    result_vec.extend_from_slice(&epk);
    for i in 15..ciphertext.len() { result_vec.push(ciphertext[i]) }

    Ok(base64::encode(result_vec))
}
/* pub fn seal(plaintext: &str, pkey_encoded: &str) -> Result<String, Box<dyn Error>> {
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
} */

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
pub struct MultilineFormatString<'a> {
    pub strings: &'a Vec<String>,
    pub postfix: Option<&'a str>
}
impl<'a> fmt::Display for MultilineFormatString<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let indent = f.width().unwrap_or(0);
        for s in self.strings {
            for _ in 0..indent {
                write!(f, " ")?;
            }
            f.write_str(s)?;
            match self.postfix {
                Some(v) => f.write_str(&format!("{}\n", v))?,
                None => f.write_str("\n")?
            }
        }
        Ok(())
    }
}
