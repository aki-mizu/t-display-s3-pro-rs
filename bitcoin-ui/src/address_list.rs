use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};

use bdk_chain::{
    bitcoin::{
        Address, Network,
        bip32::{ChildNumber, DerivationPath, Xpriv, Xpub},
        secp256k1::Secp256k1,
    },
    indexer::keychain_txout::KeychainTxOutIndex,
};
use bip39_last_word::{MnemonicLanguage, seed_from_entropy_with_empty_passphrase};
use miniscript::{
    Descriptor, DescriptorPublicKey,
    descriptor::{DescriptorXKey, Wildcard},
};
use slint::{SharedString, VecModel};

use crate::{generated::AppWindow, mnemonic::LastWordFlow};

const ADDRESSES_PER_PAGE: usize = 1;
const ADDRESSES_PER_PAGE_U32: u32 = ADDRESSES_PER_PAGE as u32;
const BIP84_ADDRESS_LINE_LENGTH: usize = 21;
const DESCRIPTOR_CHARS_PER_PAGE: usize = BIP84_ADDRESS_LINE_LENGTH * 2;
const MAX_BIP32_DERIVATION_INDEX: u32 = 0x7fff_ffff;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Keychain {
    External,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AddressListError {
    InvalidMasterKey,
    InvalidAccountPath,
    InvalidDescriptor,
    UnableToIndexDescriptor,
    UnableToRevealAddress,
    UnsupportedScript,
    MissingEntropy,
}

struct ReceiveAddressPage {
    rows: [String; ADDRESSES_PER_PAGE],
    descriptor: String,
}

impl Clone for ReceiveAddressPage {
    fn clone(&self) -> Self {
        Self {
            rows: self.rows.clone(),
            descriptor: self.descriptor.clone(),
        }
    }
}

/// Public-only state retained while browsing addresses for one finalized mnemonic.
///
/// The seed and private extended keys are used only during construction. The
/// cache retains an xpub descriptor, revealed scripts, addresses, and public
/// descriptors so subsequent navigation does not repeat BIP39 or BIP32 setup.
pub(crate) struct ReceiveAddressCache {
    descriptor: Descriptor<DescriptorPublicKey>,
    index: KeychainTxOutIndex<Keychain>,
    pages: Vec<ReceiveAddressPage>,
}

impl ReceiveAddressCache {
    fn from_flow(flow: &LastWordFlow) -> Result<Self, AddressListError> {
        let entropy = flow
            .finalized_entropy()
            .ok_or(AddressListError::MissingEntropy)?;

        Self::from_entropy(entropy, flow.mnemonic_language())
    }

    fn from_entropy(
        entropy: [u8; 16],
        language: MnemonicLanguage,
    ) -> Result<Self, AddressListError> {
        let seed = seed_from_entropy_with_empty_passphrase(language, &entropy);
        let secp = Secp256k1::signing_only();
        let master_key = Xpriv::new_master(Network::Bitcoin, &seed)
            .map_err(|_| AddressListError::InvalidMasterKey)?;
        let master_fingerprint = master_key.fingerprint(&secp);
        let account_path = bip84_account_path()?;
        let account_key = master_key
            .derive_priv(&secp, &account_path)
            .map_err(|_| AddressListError::InvalidAccountPath)?;
        let account_xpub = Xpub::from_priv(&secp, &account_key);
        let descriptor_key = DescriptorPublicKey::XPub(DescriptorXKey {
            origin: Some((master_fingerprint, account_path)),
            xkey: account_xpub,
            derivation_path: bip84_external_path()?,
            wildcard: Wildcard::Unhardened,
        });
        let descriptor = Descriptor::new_wpkh(descriptor_key)
            .map_err(|_| AddressListError::InvalidDescriptor)?;
        let mut index = KeychainTxOutIndex::new(0, false);
        index
            .insert_descriptor(Keychain::External, descriptor.clone())
            .map_err(|_| AddressListError::UnableToIndexDescriptor)?;

        Ok(Self {
            descriptor,
            index,
            pages: Vec::new(),
        })
    }

    fn page_at(&mut self, target_index: u32) -> Result<ReceiveAddressPage, AddressListError> {
        if target_index > MAX_BIP32_DERIVATION_INDEX {
            return Err(AddressListError::UnableToRevealAddress);
        }

        let (_revealed_scripts, _changeset) = self
            .index
            .reveal_to_target(Keychain::External, target_index)
            .ok_or(AddressListError::UnableToRevealAddress)?;

        let target_position =
            usize::try_from(target_index).map_err(|_| AddressListError::UnableToRevealAddress)?;
        while self.pages.len() <= target_position {
            let index = u32::try_from(self.pages.len())
                .map_err(|_| AddressListError::UnableToRevealAddress)?;
            let script = self
                .index
                .spk_at_index(Keychain::External, index)
                .ok_or(AddressListError::UnableToRevealAddress)?;
            let address = Address::from_script(&script, Network::Bitcoin)
                .map_err(|_| AddressListError::UnsupportedScript)?;
            let descriptor = self
                .descriptor
                .at_derivation_index(index)
                .map_err(|_| AddressListError::InvalidDescriptor)?;

            self.pages.push(ReceiveAddressPage {
                rows: [format_address_for_display(address.to_string())],
                descriptor: descriptor.to_string(),
            });
        }

        Ok(self.pages[target_position].clone())
    }
}

pub(crate) fn show_receive_addresses(
    window: &AppWindow,
    cache: &mut Option<ReceiveAddressCache>,
    flow: &LastWordFlow,
    start_index: u32,
) {
    if cache.is_none() {
        *cache = ReceiveAddressCache::from_flow(flow).ok();
    }

    match cache
        .as_mut()
        .ok_or(AddressListError::MissingEntropy)
        .and_then(|cache| cache.page_at(start_index))
    {
        Ok(page) => {
            let rows: Vec<SharedString> = page.rows.into_iter().map(SharedString::from).collect();
            let descriptor_pages = format_descriptor_pages(&page.descriptor);
            let descriptor_page_count = descriptor_pages.len();
            let descriptor_pages: Vec<SharedString> = descriptor_pages
                .into_iter()
                .map(SharedString::from)
                .collect();
            window.set_address_rows(VecModel::from_slice(&rows));
            window.set_address_descriptor_pages(VecModel::from_slice(&descriptor_pages));
            window.set_address_descriptor_page(0);
            window.set_address_descriptor_page_count(descriptor_page_count as i32);
            window.set_address_view(0);
            window.set_address_start_index(start_index as i32);
            window.set_address_loading(false);
            window.set_address_page_label(format!("Index {start_index}").into());
            window.set_address_has_previous_page(start_index >= ADDRESSES_PER_PAGE_U32);
            window.set_address_has_next_page(
                start_index <= MAX_BIP32_DERIVATION_INDEX - (ADDRESSES_PER_PAGE_U32 * 2 - 1),
            );
        }
        Err(_) => show_unavailable(window),
    }
}

pub(crate) fn show_address_loading(window: &AppWindow, start_index: u32) {
    let rows = [SharedString::from("Deriving BIP84 receive addresses...")];
    let descriptor_pages = [SharedString::new()];
    window.set_address_rows(VecModel::from_slice(&rows));
    window.set_address_descriptor_pages(VecModel::from_slice(&descriptor_pages));
    window.set_address_descriptor_page(0);
    window.set_address_descriptor_page_count(0);
    window.set_address_view(0);
    window.set_address_start_index(start_index as i32);
    window.set_address_loading(true);
    window.set_address_page_label("Preparing receive addresses".into());
    window.set_address_has_previous_page(false);
    window.set_address_has_next_page(false);
}

fn show_unavailable(window: &AppWindow) {
    let rows = [SharedString::from("Unable to derive receive addresses.")];
    let descriptor_pages = [SharedString::new()];
    window.set_address_rows(VecModel::from_slice(&rows));
    window.set_address_descriptor_pages(VecModel::from_slice(&descriptor_pages));
    window.set_address_descriptor_page(0);
    window.set_address_descriptor_page_count(0);
    window.set_address_view(0);
    window.set_address_start_index(0);
    window.set_address_loading(false);
    window.set_address_page_label("Unavailable".into());
    window.set_address_has_previous_page(false);
    window.set_address_has_next_page(false);
}

fn format_address_for_display(address: String) -> String {
    if address.len() == BIP84_ADDRESS_LINE_LENGTH * 2 {
        format!(
            "{}\n{}",
            &address[..BIP84_ADDRESS_LINE_LENGTH],
            &address[BIP84_ADDRESS_LINE_LENGTH..]
        )
    } else {
        address
    }
}

fn format_descriptor_pages(descriptor: &str) -> Vec<String> {
    let mut pages = Vec::with_capacity(descriptor.len().div_ceil(DESCRIPTOR_CHARS_PER_PAGE));

    for chunk in descriptor.as_bytes().chunks(DESCRIPTOR_CHARS_PER_PAGE) {
        let split_at = chunk.len().min(BIP84_ADDRESS_LINE_LENGTH);
        let first_line = core::str::from_utf8(&chunk[..split_at]).expect("descriptor is ASCII");
        let second_line = core::str::from_utf8(&chunk[split_at..]).expect("descriptor is ASCII");
        pages.push(if second_line.is_empty() {
            String::from(first_line)
        } else {
            format!("{first_line}\n{second_line}")
        });
    }

    pages
}

fn bip84_account_path() -> Result<DerivationPath, AddressListError> {
    derivation_path(&[
        ChildNumber::from_hardened_idx(84),
        ChildNumber::from_hardened_idx(0),
        ChildNumber::from_hardened_idx(0),
    ])
}

fn bip84_external_path() -> Result<DerivationPath, AddressListError> {
    derivation_path(&[ChildNumber::from_normal_idx(0)])
}

fn derivation_path(
    child_numbers: &[Result<ChildNumber, bdk_chain::bitcoin::bip32::Error>],
) -> Result<DerivationPath, AddressListError> {
    let mut path = Vec::with_capacity(child_numbers.len());
    for child_number in child_numbers {
        path.push(
            *child_number
                .as_ref()
                .map_err(|_| AddressListError::InvalidAccountPath)?,
        );
    }
    Ok(DerivationPath::from(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_the_first_bip84_receive_address() {
        let mut cache = ReceiveAddressCache::from_entropy([0; 16], MnemonicLanguage::English)
            .expect("create receive address cache");
        let page = cache.page_at(0).expect("derive BIP84 receive address zero");

        assert_eq!(page.rows.len(), 1);
        assert_eq!(page.rows[0], "bc1qcr8te4kr609gcawut\nmrza0j4xv80jy8z306fyu");
        assert!(page.descriptor.starts_with("wpkh(["));
        assert!(page.descriptor.contains("/0/0)"));
        assert!(page.descriptor.contains("#"));

        let descriptor_pages = format_descriptor_pages(&page.descriptor);
        assert!(descriptor_pages.len() > 1);
        let mut descriptor_round_trip = String::new();
        for descriptor_page in descriptor_pages {
            for character in descriptor_page.chars() {
                if character != '\n' {
                    descriptor_round_trip.push(character);
                }
            }
        }
        assert_eq!(descriptor_round_trip, page.descriptor);
    }

    #[test]
    fn binds_the_descriptor_to_the_selected_address_index() {
        let mut cache = ReceiveAddressCache::from_entropy([0; 16], MnemonicLanguage::English)
            .expect("create receive address cache");
        let page = cache.page_at(1).expect("derive BIP84 receive address one");

        assert_eq!(page.rows.len(), 1);
        assert!(page.descriptor.contains("/0/1)"));
        assert!(page.descriptor.contains("#"));
    }

    #[test]
    fn retains_revealed_pages_for_back_navigation() {
        let mut cache = ReceiveAddressCache::from_entropy([0; 16], MnemonicLanguage::English)
            .expect("create receive address cache");
        let first_page = cache.page_at(0).expect("derive receive address zero");
        let second_page = cache.page_at(1).expect("derive receive address one");
        let first_page_again = cache.page_at(0).expect("reuse receive address zero");

        assert_eq!(cache.pages.len(), 2);
        assert_eq!(cache.index.last_revealed_index(Keychain::External), Some(1));
        assert_eq!(first_page.rows, first_page_again.rows);
        assert_ne!(first_page.rows, second_page.rows);
    }
}
