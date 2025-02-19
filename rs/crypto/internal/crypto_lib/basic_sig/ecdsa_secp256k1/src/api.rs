//! ECDSA signature methods
use super::types;
use ic_crypto_internal_basic_sig_der_utils::PkixAlgorithmIdentifier;
use ic_crypto_secrets_containers::SecretVec;
use ic_types::crypto::{AlgorithmId, CryptoError, CryptoResult};
use openssl::bn::{BigNum, BigNumContext};
use openssl::ec::{EcGroup, EcKey, EcPoint};
use openssl::ecdsa::EcdsaSig;
use openssl::nid::Nid;
use openssl::pkey::PKey;
use simple_asn1::oid;

#[cfg(test)]
mod tests;

const CURVE_NAME: Nid = Nid::SECP256K1;

/// Return the algorithm identifier associated with ECDSA secp256k1
pub fn algorithm_identifier() -> PkixAlgorithmIdentifier {
    PkixAlgorithmIdentifier::new_with_oid_param(
        oid!(1, 2, 840, 10045, 2, 1),
        oid!(1, 3, 132, 0, 10),
    )
}

// NOTE: `new_keypair()` is marked as `#[cfg(test)]`
// because the focus is on the signature verification (rather than creation),
// which is the only ECDSA functionality needed currently.
// For the same reason the majority of tests is using signature verification
// test vectors (addition of test vectors for signature creation is more
// involved as Rust OpenSSL API doesn't seem to provide a way for
// "de-randomization" of signing operation).

/// Create a new secp256k1 keypair. This function should only be used for
/// testing.
///
/// # Errors
/// * `AlgorithmNotSupported` if an error occurs while generating the key
/// # Returns
/// A tuple of the secret key bytes and public key bytes
#[cfg(test)]
pub fn new_keypair() -> CryptoResult<(types::SecretKeyBytes, types::PublicKeyBytes)> {
    let group = EcGroup::from_curve_name(CURVE_NAME)
        .map_err(|e| wrap_openssl_err(e, "unable to create EC group"))?;
    let ec_key =
        EcKey::generate(&group).map_err(|e| wrap_openssl_err(e, "unable to generate EC key"))?;
    let mut ctx =
        BigNumContext::new().map_err(|e| wrap_openssl_err(e, "unable to create BigNumContext"))?;
    let mut sk_der =
        ec_key
            .private_key_to_der()
            .map_err(|e| CryptoError::AlgorithmNotSupported {
                algorithm: AlgorithmId::EcdsaSecp256k1,
                reason: format!("OpenSSL failed with error {}", e.to_string()),
            })?;
    let sk = types::SecretKeyBytes(SecretVec::new_and_zeroize_argument(&mut sk_der));
    let pk_bytes = ec_key
        .public_key()
        .to_bytes(
            &group,
            openssl::ec::PointConversionForm::UNCOMPRESSED,
            &mut ctx,
        )
        .map_err(|e| wrap_openssl_err(e, "unable to serialize EC public key"))?;
    let pk = types::PublicKeyBytes::from(pk_bytes);
    Ok((sk, pk))
}

/// Create a secp256k1 secret key from raw bytes
///
/// # Arguments
/// * `sk_raw_bytes` is the big-endian encoding of unsigned integer
/// * `pk` is the public key associated with this secret key
/// # Errors
/// * `AlgorithmNotSupported` if an error occured while invoking OpenSSL
/// * `MalformedPublicKey` if the public key could not be parsed
/// * `MalformedSecretKey` if the secret key does not coorespond with the public
///   key
pub fn secret_key_from_components(
    sk_raw_bytes: &[u8],
    pk: &types::PublicKeyBytes,
) -> CryptoResult<types::SecretKeyBytes> {
    let group = EcGroup::from_curve_name(CURVE_NAME)
        .map_err(|e| wrap_openssl_err(e, "unable to create EC group"))?;
    let private_number = BigNum::from_slice(sk_raw_bytes)
        .map_err(|e| wrap_openssl_err(e, "unable to parse big integer"))?;
    let mut ctx =
        BigNumContext::new().map_err(|e| wrap_openssl_err(e, "unable to create BigNumContext"))?;
    let public_point = EcPoint::from_bytes(&group, &pk.0, &mut ctx).map_err(|e| {
        CryptoError::MalformedPublicKey {
            algorithm: AlgorithmId::EcdsaSecp256k1,
            key_bytes: Some(pk.0.to_vec()),
            internal_error: e.to_string(),
        }
    })?;
    let ec_key =
        EcKey::from_private_components(&group, &private_number, &public_point).map_err(|_| {
            CryptoError::MalformedSecretKey {
                algorithm: AlgorithmId::EcdsaSecp256k1,
                internal_error: "OpenSSL error".to_string(), // don't leak sensitive information
            }
        })?;
    let mut sk_der =
        ec_key
            .private_key_to_der()
            .map_err(|e| CryptoError::AlgorithmNotSupported {
                algorithm: AlgorithmId::EcdsaSecp256k1,
                reason: format!("OpenSSL failed with error {}", e.to_string()),
            })?;
    Ok(types::SecretKeyBytes(SecretVec::new_and_zeroize_argument(
        &mut sk_der,
    )))
}

/// Parse a secp256k1 public key from the DER enncoding
///
/// # Arguments
/// * `pk_der` is the binary DER encoding of the public key
/// # Errors
/// * `AlgorithmNotSupported` if an error occured while invoking OpenSSL
/// * `MalformedPublicKey` if the public key could not be parsed
/// # Returns
/// The decoded public key
pub fn public_key_from_der(pk_der: &[u8]) -> CryptoResult<types::PublicKeyBytes> {
    let pkey = PKey::public_key_from_der(pk_der).map_err(|e| CryptoError::MalformedPublicKey {
        algorithm: AlgorithmId::EcdsaSecp256k1,
        key_bytes: Some(Vec::from(pk_der)),
        internal_error: e.to_string(),
    })?;
    let ec_key = pkey.ec_key().map_err(|e| CryptoError::MalformedPublicKey {
        algorithm: AlgorithmId::EcdsaSecp256k1,
        key_bytes: Some(Vec::from(pk_der)),
        internal_error: e.to_string(),
    })?;
    let mut ctx =
        BigNumContext::new().map_err(|e| wrap_openssl_err(e, "unable to create BigNumContext"))?;
    let group = EcGroup::from_curve_name(CURVE_NAME)
        .map_err(|e| wrap_openssl_err(e, "unable to create EC group"))?;
    let pk_bytes = ec_key
        .public_key()
        .to_bytes(
            &group,
            openssl::ec::PointConversionForm::UNCOMPRESSED,
            &mut ctx,
        )
        .map_err(|e| CryptoError::MalformedPublicKey {
            algorithm: AlgorithmId::EcdsaSecp256k1,
            key_bytes: Some(Vec::from(pk_der)),
            internal_error: e.to_string(),
        })?;
    // Check pk_der is in canonical form (uncompressed).
    let canon =
        public_key_to_der(&types::PublicKeyBytes::from(pk_bytes.clone())).map_err(|_e| {
            CryptoError::MalformedPublicKey {
                algorithm: AlgorithmId::EcdsaSecp256k1,
                key_bytes: Some(Vec::from(pk_der)),
                internal_error: "cannot encode decoded key".to_string(),
            }
        })?;
    if canon != pk_der {
        return Err(CryptoError::MalformedPublicKey {
            algorithm: AlgorithmId::EcdsaSecp256k1,
            key_bytes: Some(Vec::from(pk_der)),
            internal_error: "non-canonical encoding".to_string(),
        });
    }
    Ok(types::PublicKeyBytes::from(pk_bytes))
}

/// Encode a secp256k1 public key to the DER encoding
///
/// # Arguments
/// * `pk` is the public key
/// # Errors
/// * `AlgorithmNotSupported` if an error occured while invoking OpenSSL
/// * `MalformedPublicKey` if the public key seems to be invalid
/// # Returns
/// The encoded public key
pub fn public_key_to_der(pk: &types::PublicKeyBytes) -> CryptoResult<Vec<u8>> {
    let group = EcGroup::from_curve_name(CURVE_NAME)
        .map_err(|e| wrap_openssl_err(e, "unable to create EC group"))?;
    let mut ctx =
        BigNumContext::new().map_err(|e| wrap_openssl_err(e, "unable to create BigNumContext"))?;
    let point = EcPoint::from_bytes(&group, &pk.0, &mut ctx).map_err(|e| {
        CryptoError::MalformedPublicKey {
            algorithm: AlgorithmId::EcdsaSecp256k1,
            key_bytes: Some(pk.0.to_vec()),
            internal_error: e.to_string(),
        }
    })?;
    let ec_pk =
        EcKey::from_public_key(&group, &point).map_err(|e| CryptoError::MalformedPublicKey {
            algorithm: AlgorithmId::EcdsaSecp256k1,
            key_bytes: Some(pk.0.to_vec()),
            internal_error: e.to_string(),
        })?;
    ec_pk
        .public_key_to_der()
        .map_err(|e| CryptoError::MalformedPublicKey {
            algorithm: AlgorithmId::EcdsaSecp256k1,
            key_bytes: Some(pk.0.to_vec()),
            internal_error: e.to_string(),
        })
}

// Returns `secp256k1_sig` as an array of exactly types::SignatureBytes::SIZE
// bytes.
fn secp256k1_sig_to_bytes(
    secp256k1_sig: EcdsaSig,
) -> CryptoResult<[u8; types::SignatureBytes::SIZE]> {
    let r = secp256k1_sig.r().to_vec();
    let s = secp256k1_sig.s().to_vec();
    if r.len() > types::FIELD_SIZE || s.len() > types::FIELD_SIZE {
        return Err(CryptoError::MalformedSignature {
            algorithm: AlgorithmId::EcdsaSecp256k1,
            sig_bytes: secp256k1_sig
                .to_der()
                .map_err(|e| wrap_openssl_err(e, "unable to export ECDSA sig to DER format"))?,
            internal_error: "r or s is too long".to_string(),
        });
    }

    let mut bytes = [0; types::SignatureBytes::SIZE];
    // Account for leading zeros.
    bytes[(types::FIELD_SIZE - r.len())..types::FIELD_SIZE].clone_from_slice(&r);
    bytes[(types::SignatureBytes::SIZE - s.len())..types::SignatureBytes::SIZE]
        .clone_from_slice(&s);
    Ok(bytes)
}

/// Sign a message using a secp256k1 private key
///
/// # Arguments
/// * `msg` is the message to be signed
/// * `sk` is the private key
/// # Errors
/// * `InvalidArgument` if signature generation failed
/// * `MalformedSecretKey` if the private key seems to be invalid
/// * `MalformedSignature` if OpenSSL generated an invalid ECDSA signature
/// # Returns
/// The generated signature
pub fn sign(msg: &[u8], sk: &types::SecretKeyBytes) -> CryptoResult<types::SignatureBytes> {
    let signing_key = EcKey::private_key_from_der(&sk.0.expose_secret()).map_err(|_| {
        CryptoError::MalformedSecretKey {
            algorithm: AlgorithmId::EcdsaSecp256k1,
            internal_error: "OpenSSL error".to_string(), // don't leak sensitive information
        }
    })?;
    let secp256k1_sig =
        EcdsaSig::sign(msg, &signing_key).map_err(|e| CryptoError::InvalidArgument {
            message: format!("ECDSA signing failed with error {}", e),
        })?;
    let sig_bytes = secp256k1_sig_to_bytes(secp256k1_sig)?;
    Ok(types::SignatureBytes(sig_bytes))
}

// Extracts 'r' and 's' parts of a signature from `SignatureBytes'
fn r_s_from_sig_bytes(sig_bytes: &types::SignatureBytes) -> CryptoResult<(BigNum, BigNum)> {
    if sig_bytes.0.len() != types::SignatureBytes::SIZE {
        return Err(CryptoError::MalformedSignature {
            algorithm: AlgorithmId::EcdsaSecp256k1,
            sig_bytes: sig_bytes.0.to_vec(),
            internal_error: format!(
                "Expected {} bytes, got {}",
                types::SignatureBytes::SIZE,
                sig_bytes.0.len()
            ),
        });
    }
    let r = BigNum::from_slice(&sig_bytes.0[0..types::FIELD_SIZE]).map_err(|e| {
        CryptoError::MalformedSignature {
            algorithm: AlgorithmId::EcdsaSecp256k1,
            sig_bytes: sig_bytes.0.to_vec(),
            internal_error: format!("Error parsing r: {}", e.to_string()),
        }
    })?;
    let s = BigNum::from_slice(&sig_bytes.0[types::FIELD_SIZE..]).map_err(|e| {
        CryptoError::MalformedSignature {
            algorithm: AlgorithmId::EcdsaSecp256k1,
            sig_bytes: sig_bytes.0.to_vec(),
            internal_error: format!("Error parsing s: {}", e.to_string()),
        }
    })?;
    Ok((r, s))
}

/// Verify a signature using a secp256k1 public key
///
/// # Arguments
/// * `sig` is the signature to be verified
/// * `msg` is the message
/// * `pk` is the public key
/// # Errors
/// * `MalformedSignature` if the signature could not be parsed
/// * `AlgorithmNotSupported` if an error occurred while invoking OpenSSL
/// * `MalformedPublicKey` if the public key could not be parsed
/// * `SignatureVerification` if the signature could not be verified
/// # Returns
/// `Ok(())` if the signature validated, or an error otherwise
pub fn verify(
    sig: &types::SignatureBytes,
    msg: &[u8],
    pk: &types::PublicKeyBytes,
) -> CryptoResult<()> {
    let (r, s) = r_s_from_sig_bytes(sig)?;
    let secp256k1_sig =
        EcdsaSig::from_private_components(r, s).map_err(|e| CryptoError::MalformedSignature {
            algorithm: AlgorithmId::EcdsaSecp256k1,
            sig_bytes: sig.0.to_vec(),
            internal_error: e.to_string(),
        })?;
    let group = EcGroup::from_curve_name(CURVE_NAME)
        .map_err(|e| wrap_openssl_err(e, "unable to create EC group"))?;
    let mut ctx =
        BigNumContext::new().map_err(|e| wrap_openssl_err(e, "unable to create BigNumContext"))?;
    let point = EcPoint::from_bytes(&group, &pk.0, &mut ctx).map_err(|e| {
        CryptoError::MalformedPublicKey {
            algorithm: AlgorithmId::EcdsaSecp256k1,
            key_bytes: Some(pk.0.to_vec()),
            internal_error: e.to_string(),
        }
    })?;
    let ec_pk =
        EcKey::from_public_key(&group, &point).map_err(|e| CryptoError::MalformedPublicKey {
            algorithm: AlgorithmId::EcdsaSecp256k1,
            key_bytes: Some(pk.0.to_vec()),
            internal_error: e.to_string(),
        })?;
    let verified =
        secp256k1_sig
            .verify(msg, &ec_pk)
            .map_err(|e| CryptoError::SignatureVerification {
                algorithm: AlgorithmId::EcdsaSecp256k1,
                public_key_bytes: pk.0.to_vec(),
                sig_bytes: sig.0.to_vec(),
                internal_error: e.to_string(),
            })?;
    if verified {
        Ok(())
    } else {
        Err(CryptoError::SignatureVerification {
            algorithm: AlgorithmId::EcdsaSecp256k1,
            public_key_bytes: pk.0.to_vec(),
            sig_bytes: sig.0.to_vec(),
            internal_error: "verification failed".to_string(),
        })
    }
}

fn wrap_openssl_err(e: openssl::error::ErrorStack, err_msg: &str) -> CryptoError {
    CryptoError::AlgorithmNotSupported {
        algorithm: AlgorithmId::EcdsaSecp256k1,
        reason: format!("{}: OpenSSL failed with error {}", err_msg, e.to_string()),
    }
}
