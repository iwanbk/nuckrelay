use anyhow::{Result, anyhow};
use crypto::digest::Digest;
use crypto::hmac::Hmac;
use crypto::mac::Mac;
use crypto::sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};

/// Computes the SHA-256 hash of the input data.
/// This is equivalent to Go's sha256.Sum256 function.
pub fn sha256_sum(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.input(data);
    let mut result = [0u8; 32];
    hasher.result(&mut result);
    result
}

const MIN_LENGTH_UNIX_TIMESTAMP: usize = 10;

/// Token authentication algorithm
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AuthAlgo {
    Unknown = 0,
    HmacSha256 = 1,
}

impl AuthAlgo {
    /// Returns the size of the algorithm's output in bytes
    pub fn size(&self) -> usize {
        match self {
            AuthAlgo::HmacSha256 => 32, // SHA-256 produces 32 bytes
            _ => 0,
        }
    }

    /// Parses an algorithm from a byte
    pub fn from_u8(val: u8) -> Self {
        match val {
            1 => AuthAlgo::HmacSha256,
            _ => AuthAlgo::Unknown,
        }
    }
}

/// Token structure for authentication
#[derive(Debug)]
pub struct Token {
    pub auth_algo: AuthAlgo,
    pub signature: Vec<u8>,
    pub payload: Vec<u8>,
}

impl Token {
    /// Unmarshals a token from binary data
    pub fn unmarshal(data: &[u8]) -> Result<Self> {
        if data.is_empty() {
            return Err(anyhow!("invalid token data"));
        }

        let algo = AuthAlgo::from_u8(data[0]);

        let sig_size = algo.size();

        if data.len() < 1 + sig_size {
            return Err(anyhow!("invalid token data: insufficient length"));
        }

        Ok(Token {
            auth_algo: algo,
            signature: data[1..1 + sig_size].to_vec(),
            payload: data[1 + sig_size..].to_vec(),
        })
    }
}

/// Validator for HMAC tokens
#[derive(Debug, Clone)]
pub struct Validator {
    secret: Vec<u8>,
}

impl Validator {
    /// Creates a new validator with the given secret
    pub fn new(secret: Vec<u8>) -> Self {
        Validator { secret }
    }

    /// Validates the given data as a token
    pub fn validate(&self, data: &[u8]) -> Result<()> {
        // Parse the token from the binary data
        let token = Token::unmarshal(data)?;

        // Check payload length
        if token.payload.len() < MIN_LENGTH_UNIX_TIMESTAMP {
            return Err(anyhow!("invalid payload: insufficient length"));
        }

        // Get the hashing function based on the algorithm
        let mut hmac = match token.auth_algo {
            AuthAlgo::HmacSha256 => Hmac::new(Sha256::new(), &self.secret),
            _ => return Err(anyhow!("unsupported auth algorithm")),
        };

        // Compute the expected MAC
        hmac.input(&token.payload);
        let expected_mac = hmac.result();

        // Compare the MACs
        if !constant_time_eq(&token.signature, expected_mac.code()) {
            return Err(anyhow!("invalid signature"));
        }

        // Parse the timestamp from the payload
        let timestamp_str = std::str::from_utf8(&token.payload)
            .map_err(|_| anyhow!("invalid payload: not valid UTF-8"))?;

        let timestamp: i64 = timestamp_str
            .parse()
            .map_err(|_| anyhow!("invalid payload: not a valid timestamp"))?;

        // Check if the token is expired
        let current_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| anyhow!("system time error"))?
            .as_secs() as i64;

        if current_time > timestamp {
            return Err(anyhow!("expired token"));
        }

        Ok(())
    }
}

/// Constant-time comparison of two slices to prevent timing attacks
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let mut result = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }

    result == 0
}
