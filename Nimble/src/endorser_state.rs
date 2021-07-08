use crate::errors::EndorserError;
use crate::ledger::{MetaBlock, NimbleDigest, NimbleHashTrait};
use ed25519_dalek::{Keypair, PublicKey, Signature, Signer};
use rand::rngs::OsRng;
use std::collections::HashMap;

/// Endorser's internal state
pub struct EndorserState {
  /// a key pair in the ed25519 digital signature scheme
  keypair: Keypair,

  /// a map from fixed-sized labels to a tail hash and a counter
  ledgers: HashMap<NimbleDigest, (NimbleDigest, usize)>,
}

impl EndorserState {
  pub fn new() -> Self {
    let mut csprng = OsRng {};
    let keypair = Keypair::generate(&mut csprng);
    EndorserState {
      keypair,
      ledgers: HashMap::new(),
    }
  }

  pub fn new_ledger(
    &mut self,
    handle: &NimbleDigest,
    tail_hash: &NimbleDigest,
  ) -> Result<Signature, EndorserError> {
    if self.ledgers.contains_key(handle) {
      Err(EndorserError::LedgerExists)
    } else {
      self
        .ledgers
        .insert(handle.clone(), (tail_hash.clone(), 0usize));

      let signature = self.keypair.sign(tail_hash.to_bytes().as_slice());
      Ok(signature)
    }
  }

  pub fn read_latest(
    &self,
    handle: &NimbleDigest,
    nonce: &[u8],
  ) -> Result<(Vec<u8>, usize, Signature), EndorserError> {
    if !self.ledgers.contains_key(handle) {
      Err(EndorserError::InvalidLedgerName)
    } else {
      let (tail_hash_bytes, height) = self.ledgers.get(handle).unwrap(); //safe to unwrap here because of the check above
      let signature = self
        .keypair
        .sign(&[tail_hash_bytes.to_bytes(), nonce.to_vec()].concat());
      Ok((tail_hash_bytes.to_bytes(), *height, signature))
    }
  }

  pub fn append(
    &mut self,
    handle: &NimbleDigest,
    block_hash: &NimbleDigest,
    conditional_tail_hash: &NimbleDigest,
  ) -> Result<(Vec<u8>, usize, Signature), EndorserError> {
    if self.ledgers.contains_key(handle) {
      let (tail_hash, height) = self.ledgers.get_mut(handle).unwrap();

      if tail_hash != conditional_tail_hash {
        return Err(EndorserError::TailDoesNotMatch);
      }

      *height += 1;

      // save the previous tail
      let prev_tail = tail_hash.clone();

      let metadata = MetaBlock::new(&prev_tail, block_hash, *height);
      *tail_hash = metadata.hash();

      let signature = self.keypair.sign(&tail_hash.to_bytes());

      Ok((prev_tail.to_bytes(), *height, signature))
    } else {
      Err(EndorserError::StateCreationError)
    }
  }

  pub fn get_public_key(&self) -> PublicKey {
    self.keypair.public
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use rand::Rng;

  #[test]
  pub fn check_endorser_state_creation() {
    let endorser_state = EndorserState::new();
    let key_information = endorser_state.keypair;
    let public_key = key_information.public.to_bytes();
    let secret_key = key_information.secret.to_bytes();
    assert_eq!(public_key.len(), 32usize);
    assert_eq!(secret_key.len(), 32usize);
  }

  #[test]
  pub fn check_endorser_new_ledger_and_get_tail() {
    let mut endorser_state = EndorserState::new();
    // The coordinator sends the hashed contents of the block to the
    let coordinator_handle = {
      let t = rand::thread_rng().gen::<[u8; 32]>();
      let n = NimbleDigest::from_bytes(&t);
      if !n.is_ok() {
        panic!("Should not have occured");
      }
      n.unwrap()
    };
    let genesis_tail_hash = {
      let t = rand::thread_rng().gen::<[u8; 32]>();
      let n = NimbleDigest::from_bytes(&t);
      if !n.is_ok() {
        panic!("Should not have occured");
      }
      n.unwrap()
    };
    let create_ledger_endorser_response =
      endorser_state.new_ledger(&coordinator_handle, &genesis_tail_hash);
    if create_ledger_endorser_response.is_ok() {
      let signature = create_ledger_endorser_response.unwrap();
      let signature_expected = endorser_state.keypair.sign(&genesis_tail_hash.to_bytes());
      assert_eq!(signature, signature_expected);

      // Fetch the value currently in the tail.
      let tail_result = endorser_state.read_latest(&coordinator_handle, &vec![0]);
      if tail_result.is_ok() {
        let (tail_hash_data, height, _signature) = tail_result.unwrap();
        assert_eq!(height, 0usize);
        let tail_hash = NimbleDigest::from_bytes(&tail_hash_data).unwrap();
        assert_eq!(tail_hash, genesis_tail_hash);
      } else {
        panic!("Failed to retrieve correct tail hash on genesis ledger state creation");
      }
    } else {
      panic!("Failed to create ledger using genesis hash at the ledger");
    }
  }

  #[test]
  pub fn check_endorser_append_ledger_tail() {
    let mut endorser_state = EndorserState::new();

    // The coordinator sends the hashed contents of the block to the
    let coordinator_handle_data = rand::thread_rng().gen::<[u8; 32]>();
    let coordinator_handle = NimbleDigest::from_bytes(&coordinator_handle_data).unwrap();
    let genesis_tail_hash_data = rand::thread_rng().gen::<[u8; 32]>();
    let genesis_tail_hash = NimbleDigest::from_bytes(&genesis_tail_hash_data).unwrap();
    let create_ledger_endorser_response =
      endorser_state.new_ledger(&coordinator_handle, &genesis_tail_hash);

    assert!(create_ledger_endorser_response.is_ok());
    let _signature = create_ledger_endorser_response.unwrap();

    // Fetch the value currently in the tail.
    let nonce_data = rand::thread_rng().gen::<[u8; 16]>();
    let (tail_result_data, height, _signature) = endorser_state
      .read_latest(&coordinator_handle, &nonce_data)
      .unwrap();

    let block_hash_to_append_data = rand::thread_rng().gen::<[u8; 32]>();
    let block_hash_to_append = NimbleDigest::from_bytes(&block_hash_to_append_data).unwrap();
    let tail_result = NimbleDigest::from_bytes(&tail_result_data).unwrap();

    let (previous_tail_data, new_ledger_height, signature) = endorser_state
      .append(&coordinator_handle, &block_hash_to_append, &tail_result)
      .unwrap();

    let previous_tail = NimbleDigest::from_bytes(&previous_tail_data).unwrap();

    assert_eq!(tail_result, previous_tail);
    assert_eq!(new_ledger_height, height + 1);

    let metadata = MetaBlock::new(&previous_tail, &block_hash_to_append, new_ledger_height);

    let endorser_tail_expectation = metadata.hash();

    let tail_signature_verification = endorser_state
      .keypair
      .verify(&endorser_tail_expectation.to_bytes(), &signature);

    if tail_signature_verification.is_ok() {
      println!("Verification Passed. Checking Updated Tail");
      let (tail_result_data, _height, _signature) = endorser_state
        .read_latest(&coordinator_handle, &vec![0])
        .unwrap();
      let tail_result = NimbleDigest::from_bytes(&tail_result_data).unwrap();

      assert_eq!(endorser_tail_expectation, tail_result);
    } else {
      panic!("Signature verification failed when it should not have failed");
    }
  }
}
