//! Magazyn kluczy chroniony hasłem: `keys/node-key.enc` (klucz węzła, Argon2id +
//! XChaCha20-Poly1305) i `keys/pgp-secret.asc` (klucz OpenPGP chroniony natywnie, S2K).
use crate::consts::{
    ARGON2_SALT_BYTES, ENVELOPE_VERSION, MIN_PASSWORD_CHARS, NODE_KEY_FILE, PGP_PUBLIC_FILE,
    PGP_SECRET_FILE, XCHACHA_NONCE_BYTES,
};
use crate::error::SecurityError;
use crate::identity::did_from_public;
use crate::models::{EncryptedKeyFile, Identity};
use crate::utils::{b64, unb64};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use libp2p::PeerId;
use libp2p::identity::Keypair;
use pgp::composed::{
    ArmorOptions, Deserializable, EncryptionCaps, KeyType, SecretKeyParamsBuilder, SignedPublicKey,
    SignedSecretKey, SubkeyParamsBuilder,
};
use pgp::crypto::ecc_curve::ECCCurve;
use pgp::crypto::hash::HashAlgorithm;
use pgp::crypto::sym::SymmetricKeyAlgorithm;
use std::fs;
use std::io::Write;
use std::path::Path;
use zeroize::Zeroizing;

impl Identity {
    pub fn exists(directory: &Path) -> bool {
        directory.join(NODE_KEY_FILE).is_file()
    }

    /// Tworzy klucz węzła i klucz OpenPGP; nie nadpisuje istniejących.
    pub fn create(directory: &Path, password: &str) -> Result<Self, SecurityError> {
        if password.chars().count() < MIN_PASSWORD_CHARS {
            return Err(SecurityError::WeakPassword(MIN_PASSWORD_CHARS));
        }
        if Self::exists(directory) {
            return Err(SecurityError::KeysExist(directory.display().to_string()));
        }
        create_private_dir(directory)?;
        let keypair = Keypair::generate_ed25519();
        let did = did_from_public(&keypair.public())?;
        let pgp_secret = generate_pgp(&did, password)?;
        let pgp_public = SignedPublicKey::from(pgp_secret.clone());
        write_private(
            &directory.join(PGP_SECRET_FILE),
            pgp_secret
                .to_armored_string(ArmorOptions::default())?
                .as_bytes(),
        )?;
        write_private(
            &directory.join(PGP_PUBLIC_FILE),
            pgp_public
                .to_armored_string(ArmorOptions::default())?
                .as_bytes(),
        )?;
        // Klucz węzła na końcu: jego obecność oznacza kompletny magazyn (`exists`).
        write_private(
            &directory.join(NODE_KEY_FILE),
            &serde_json::to_vec(&seal(&keypair, password)?)?,
        )?;
        Ok(Self {
            keypair,
            pgp_secret,
            pgp_public,
            password: Zeroizing::new(password.to_owned()),
        })
    }

    pub fn unlock(directory: &Path, password: &str) -> Result<Self, SecurityError> {
        if !Self::exists(directory) {
            return Err(SecurityError::NoKeys(directory.display().to_string()));
        }
        let sealed: EncryptedKeyFile =
            serde_json::from_slice(&fs::read(directory.join(NODE_KEY_FILE))?)?;
        let keypair = open(&sealed, password)?;
        let armored = fs::read_to_string(directory.join(PGP_SECRET_FILE))?;
        let (pgp_secret, _) = SignedSecretKey::from_armor_single(armored.as_bytes())?;
        pgp_secret.verify_bindings()?;
        // Hasło klucza OpenPGP sprawdza próbne odblokowanie klucza głównego.
        pgp_secret
            .primary_key
            .unlock(&password.into(), |_, _| Ok(()))
            .map_err(|_| SecurityError::WrongPassword)?
            .map_err(|_: pgp::errors::Error| SecurityError::WrongPassword)?;
        let pgp_public = SignedPublicKey::from(pgp_secret.clone());
        Ok(Self {
            keypair,
            pgp_secret,
            pgp_public,
            password: Zeroizing::new(password.to_owned()),
        })
    }

    pub fn did(&self) -> String {
        did_from_public(&self.keypair.public()).expect("identity keys are Ed25519")
    }

    pub fn peer_id(&self) -> PeerId {
        PeerId::from(self.keypair.public())
    }

    pub fn pgp_public_armored(&self) -> Result<String, SecurityError> {
        Ok(self.pgp_public.to_armored_string(ArmorOptions::default())?)
    }
}

/// Ed25519 (podpis) + Curve25519 ECDH (szyfrowanie), klucze v4: format, który czyta GnuPG.
fn generate_pgp(user_id: &str, password: &str) -> Result<SignedSecretKey, SecurityError> {
    let build_error = |error: &dyn std::fmt::Display| SecurityError::Key(error.to_string());
    let mut encryption = SubkeyParamsBuilder::default();
    encryption
        .key_type(KeyType::ECDH(ECCCurve::Curve25519Legacy))
        .can_sign(false)
        .can_encrypt(EncryptionCaps::All)
        .passphrase(Some(password.to_owned()));
    let mut params = SecretKeyParamsBuilder::default();
    params
        .key_type(KeyType::Ed25519Legacy)
        .can_certify(true)
        .can_sign(true)
        .can_encrypt(EncryptionCaps::None)
        .primary_user_id(user_id.to_owned())
        // Bez preferencji GnuPG podpisuje SHA-1, którego rPGP nie przyjmuje dla Ed25519.
        .preferred_hash_algorithms(
            [HashAlgorithm::Sha256, HashAlgorithm::Sha512]
                .into_iter()
                .collect(),
        )
        .preferred_symmetric_algorithms(
            [SymmetricKeyAlgorithm::AES256, SymmetricKeyAlgorithm::AES128]
                .into_iter()
                .collect(),
        )
        .passphrase(Some(password.to_owned()))
        .subkeys(vec![encryption.build().map_err(|e| build_error(&e))?]);
    let params = params.build().map_err(|e| build_error(&e))?;
    Ok(params.generate(rand::thread_rng())?)
}

fn derive_key(password: &str, salt: &[u8]) -> Result<Zeroizing<[u8; 32]>, SecurityError> {
    let mut key = Zeroizing::new([0u8; 32]);
    argon2::Argon2::default()
        .hash_password_into(password.as_bytes(), salt, key.as_mut())
        .map_err(|error| SecurityError::Key(error.to_string()))?;
    Ok(key)
}

fn seal(keypair: &Keypair, password: &str) -> Result<EncryptedKeyFile, SecurityError> {
    let salt: [u8; ARGON2_SALT_BYTES] = rand::random();
    let nonce: [u8; XCHACHA_NONCE_BYTES] = rand::random();
    let plain = Zeroizing::new(
        keypair
            .to_protobuf_encoding()
            .map_err(|error| SecurityError::Key(error.to_string()))?,
    );
    let key = derive_key(password, &salt)?;
    let ciphertext = XChaCha20Poly1305::new(key.as_ref().into())
        .encrypt(XNonce::from_slice(&nonce), plain.as_slice())
        .map_err(|_| SecurityError::Key("encryption failed".into()))?;
    Ok(EncryptedKeyFile {
        v: ENVELOPE_VERSION,
        kdf: "argon2id".into(),
        salt: b64(&salt),
        nonce: b64(&nonce),
        ciphertext: b64(&ciphertext),
    })
}

fn open(sealed: &EncryptedKeyFile, password: &str) -> Result<Keypair, SecurityError> {
    let nonce = unb64(&sealed.nonce)?;
    if sealed.kdf != "argon2id" || nonce.len() != XCHACHA_NONCE_BYTES {
        return Err(SecurityError::WrongPassword);
    }
    let key = derive_key(password, &unb64(&sealed.salt)?)?;
    let plain = Zeroizing::new(
        XChaCha20Poly1305::new(key.as_ref().into())
            .decrypt(
                XNonce::from_slice(&nonce),
                unb64(&sealed.ciphertext)?.as_slice(),
            )
            .map_err(|_| SecurityError::WrongPassword)?,
    );
    Keypair::from_protobuf_encoding(&plain).map_err(|error| SecurityError::Key(error.to_string()))
}

fn create_private_dir(directory: &Path) -> Result<(), SecurityError> {
    fs::create_dir_all(directory)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), SecurityError> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
