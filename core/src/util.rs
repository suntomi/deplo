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

// fs
use std::fs;
use std::path::Path;
use fs_extra;
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
        Ok(_) => log::info!(
            "infra setup scripts for {}({:?}) already copied",
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

use crc::{Crc, CRC_64_ECMA_182};

const crc: Crc<u64> = Crc::<u64>::new(&CRC_64_ECMA_182);

pub fn maphash(h: &HashMap<String, String>) -> String {
    // hash value for separating repository cache according to checkout options
    let mut v: Vec<_> = h.into_iter().collect();
    let mut digest = crc.digest();
    v.sort_by(|x,y| x.0.cmp(&y.0));
    digest.update(v.iter().map(|(k,v)| format!("{}={}", k, v)).collect::<Vec<String>>().join(",").as_bytes());
    format!("{:X}", digest.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal() {
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
}
