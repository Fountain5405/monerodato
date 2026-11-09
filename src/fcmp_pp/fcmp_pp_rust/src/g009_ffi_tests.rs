// G009 FFI gate test: fcmp_pp_verify_prepare / _finish / destroy against the
// real tests/data vectors. Extracted from the second reviewer's in-tree module.

// This file IS the g009_ffi_tests module (declared #[cfg(test)] in lib.rs).
use super::{
    destroy_fcmp_pp_batch_state, fcmp_pp_verify_finish, fcmp_pp_verify_prepare,
    FcmpPpBatchState, FcmpPpVerifyInput, Slice,
  };
  use ciphersuite::Ciphersuite;
  use dalek_ff_group::Ed25519;
  use helioselene::{Helios, Selene};
  use monero_fcmp_plus_plus::{FcmpPlusPlus, Curves};
  use full_chain_membership_proofs::TreeRoot;

  // Parse tests/data/fcmp_pp_verify_inputs_{n}in.bin (monero binary serialization:
  // hash(32) | layers(u8) | proof blob(varint len + bytes) | tree_root(32) |
  // pseudo_outs(varint count + 32 each) | key_images(varint count + 32 each)).
  fn parse_vector(n_inputs: usize) -> ([u8; 32], u8, Vec<u8>, [u8; 32], Vec<[u8; 32]>, Vec<[u8; 32]>) {
    let name = format!(
      "{}/../../../tests/data/fcmp_pp_verify_inputs_{}in.bin",
      env!("CARGO_MANIFEST_DIR"),
      n_inputs
    );
    let blob = std::fs::read(&name).unwrap_or_else(|e| panic!("read {name}: {e}"));
    let mut i = 0usize;
    let mut take = |i: &mut usize, n: usize| -> Vec<u8> {
      let v = blob[*i..*i + n].to_vec();
      *i += n;
      v
    };
    let mut varint = |i: &mut usize| -> u64 {
      let mut res = 0u64;
      let mut shift = 0;
      loop {
        let b = blob[*i];
        *i += 1;
        res |= ((b & 0x7f) as u64) << shift;
        if b & 0x80 == 0 {
          break;
        }
        shift += 7;
      }
      res
    };
    let hash_bytes: [u8; 32] = take(&mut i, 32).try_into().unwrap();
    let layers = blob[i];
    i += 1;
    let proof_len = varint(&mut i) as usize;
    let proof = take(&mut i, proof_len);
    let root: [u8; 32] = take(&mut i, 32).try_into().unwrap();
    let n_pseudo = varint(&mut i) as usize;
    let pseudo_outs: Vec<[u8; 32]> = (0..n_pseudo)
      .map(|_| take(&mut i, 32).try_into().unwrap())
      .collect();
    let n_ki = varint(&mut i) as usize;
    let key_images: Vec<[u8; 32]> = (0..n_ki)
      .map(|_| take(&mut i, 32).try_into().unwrap())
      .collect();
    assert_eq!(i, blob.len(), "trailing bytes in vector file");
    assert_eq!(pseudo_outs.len(), n_inputs);
    assert_eq!(key_images.len(), n_inputs);
    assert_eq!(
      proof.len(),
      FcmpPlusPlus::proof_size(n_inputs, layers as usize),
      "proof length mismatch"
    );
    (hash_bytes, layers, proof, root, pseudo_outs, key_images)
  }

  fn make_input(
    (hash, layers, proof, root, pseudo_outs, key_images): &(
      [u8; 32],
      u8,
      Vec<u8>,
      [u8; 32],
      Vec<[u8; 32]>,
      Vec<[u8; 32]>,
    ),
    mangle_layers: bool,
  ) -> FcmpPpVerifyInput {
    let mut proof: &[u8] = proof;
    let fcmp_pp = FcmpPlusPlus::read(pseudo_outs, *layers as usize, &mut proof).unwrap();
    let mut root_bytes: &[u8] = root;
    let tree_root = if layers % 2 == 0 {
      TreeRoot::C2(<Helios as Ciphersuite>::read_G(&mut root_bytes).unwrap())
    } else {
      TreeRoot::C1(<Selene as Ciphersuite>::read_G(&mut root_bytes).unwrap())
    };
    let key_images = key_images
      .iter()
      .map(|b| {
        let mut b: &[u8] = b;
        <Ed25519 as Ciphersuite>::read_G(&mut b).unwrap()
      })
      .collect();
    FcmpPpVerifyInput {
      fcmp_pp,
      tree_root,
      n_tree_layers: if mangle_layers { *layers as usize + 1 } else { *layers as usize },
      signable_tx_hash: *hash,
      key_images,
    }
  }

  fn prepare_batch(batch: &[FcmpPpVerifyInput]) -> *mut FcmpPpBatchState {
    let ptrs: Vec<*const FcmpPpVerifyInput> = batch.iter().map(|x| x as *const _).collect();
    unsafe {
      fcmp_pp_verify_prepare(Slice {
        buf: ptrs.as_ptr() as *const *const FcmpPpVerifyInput,
        len: ptrs.len(),
      })
    }
  }

  fn finish_batches(states: &mut Vec<*mut FcmpPpBatchState>, split: usize) -> bool {
    unsafe {
      let ok = fcmp_pp_verify_finish(
        Slice { buf: states.as_mut_ptr(), len: states.len() },
        split,
      );
      states.fill(std::ptr::null_mut());
      ok
    }
  }

  fn free_states(states: &mut Vec<*mut FcmpPpBatchState>) {
    for s in states.iter_mut() {
      unsafe { destroy_fcmp_pp_batch_state(*s) };
      *s = std::ptr::null_mut();
    }
  }

  #[test]
  fn g009_ffi_valid_merges_and_rejections() {
    let v1 = parse_vector(1);
    let v2 = parse_vector(2);

    // Case 1: several batches of valid proofs, split 1 and >1
    for split in [1usize, 4] {
      let mut states = vec![];
      for batch in [vec![make_input(&v1, false)], vec![make_input(&v2, false)], vec![make_input(&v1, false), make_input(&v2, false)]] {
        let s = prepare_batch(&batch);
        assert!(!s.is_null(), "valid batch must prepare non-null");
        states.push(s);
      }
      assert!(finish_batches(&mut states, split), "merged valid batches must verify (split {split})");
    }

    // Case 2: a malleated proof (still parsing) among valid batches: prepare stays
    // non-null (per-proof checks only queue), the merged finish must reject.
    let mut malleated = v1.clone();
    // Flip a byte in the middle of the FCMP section (after the SAL parts)
    let fcmp_offset = 1 * ((3 * 32) + (12 * 32));
    let idx = fcmp_offset + 64.min(malleated.2.len() - fcmp_offset - 1);
    malleated.2[idx] ^= 0x01;
    {
      let bad_input = make_input(&malleated, false);
      let mut states = vec![];
      for batch in [vec![make_input(&v2, false)], vec![bad_input], vec![make_input(&v2, false)]] {
        let s = prepare_batch(&batch);
        assert!(!s.is_null(), "parseable malleation must still queue (non-null state)");
        states.push(s);
      }
      assert!(!finish_batches(&mut states, 1), "merged malleated batch must fail");
    }
    // and the same under a split
    {
      let bad_input = make_input(&malleated, false);
      let mut states = vec![];
      for batch in [vec![make_input(&v2, false)], vec![bad_input], vec![make_input(&v2, false)]] {
        states.push(prepare_batch(&batch));
      }
      assert!(!finish_batches(&mut states, 4), "merged malleated batch must fail (split)");
    }

    // Case 3: a per-proof failing input (layers mismatch -> Err in verify) makes
    // prepare return null; the already-prepared states are then freed with
    // destroy, twice-guarded, and must not leak or double-free.
    let mut states = vec![];
    states.push(prepare_batch(&[make_input(&v2, false)]));
    let failing = make_input(&v1, true);
    assert!(prepare_batch(&[failing]).is_null(), "per-proof failure must return null");
    free_states(&mut states);
    // destroy on an already-nulled slot must be a no-op
    free_states(&mut states);
  }
