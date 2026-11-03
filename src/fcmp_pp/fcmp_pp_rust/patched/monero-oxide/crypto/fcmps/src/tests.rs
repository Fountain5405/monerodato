use rand_core::OsRng;

use generic_array::typenum::U;

use multiexp::multiexp_vartime;
use ciphersuite::Ciphersuite;
use dalek_ff_group::Ed25519;
use helioselene::{Selene, Helios};
use ec_divisors::ScalarDecomposition;

use crate::{*, tree::hash_grow};

struct Ed25519Params;
impl DiscreteLogParameter for Ed25519Params {
  #[allow(clippy::as_conversions)]
  type ScalarBits = U<{ <<Ed25519 as Ciphersuite>::F as PrimeField>::NUM_BITS as usize }>;
}

struct SeleneParams;
impl DiscreteLogParameter for SeleneParams {
  #[allow(clippy::as_conversions)]
  type ScalarBits = U<{ <<Selene as Ciphersuite>::F as PrimeField>::NUM_BITS as usize }>;
}

struct HeliosParams;
impl DiscreteLogParameter for HeliosParams {
  #[allow(clippy::as_conversions)]
  type ScalarBits = U<{ <<Helios as Ciphersuite>::F as PrimeField>::NUM_BITS as usize }>;
}

#[derive(Clone)]
struct MoneroCurves;
impl FcmpCurves for MoneroCurves {
  type OC = Ed25519;
  type OcParameters = Ed25519Params;
  type C1 = Selene;
  type C1Parameters = SeleneParams;
  type C2 = Helios;
  type C2Parameters = HeliosParams;
}

#[allow(clippy::type_complexity)]
fn random_params(
  input_limit: usize,
) -> (
  <Ed25519 as Ciphersuite>::G,
  <Ed25519 as Ciphersuite>::G,
  <Ed25519 as Ciphersuite>::G,
  <Ed25519 as Ciphersuite>::G,
  FcmpParams<MoneroCurves>,
) {
  let G = <Ed25519 as Ciphersuite>::G::random(&mut OsRng);
  let T = <Ed25519 as Ciphersuite>::G::random(&mut OsRng);
  let U = <Ed25519 as Ciphersuite>::G::random(&mut OsRng);
  let V = <Ed25519 as Ciphersuite>::G::random(&mut OsRng);

  let params = FcmpParams::<MoneroCurves>::new(
    generalized_bulletproofs::tests::insecure_test_generators::<_, Selene>(
      &mut OsRng,
      2 * (input_limit * 256),
    )
    .unwrap(),
    generalized_bulletproofs::tests::insecure_test_generators::<_, Helios>(
      &mut OsRng,
      2 * (input_limit * 128),
    )
    .unwrap(),
    // Hash init generators
    <Selene as Ciphersuite>::G::random(&mut OsRng),
    <Helios as Ciphersuite>::G::random(&mut OsRng),
    // G, T, U, V
    G,
    T,
    U,
    V,
  );

  (G, T, U, V, params)
}

fn random_output() -> Output<<Ed25519 as Ciphersuite>::G> {
  let O = <Ed25519 as Ciphersuite>::G::random(&mut OsRng);
  let I = <Ed25519 as Ciphersuite>::G::random(&mut OsRng);
  let C = <Ed25519 as Ciphersuite>::G::random(&mut OsRng);
  Output::new(O, I, C).unwrap()
}

fn random_path(
  params: &FcmpParams<MoneroCurves>,
  layers: usize,
) -> (Path<MoneroCurves>, TreeRoot<Selene, Helios>) {
  assert!(layers >= 1);

  let mut leaves = vec![];
  while leaves.len() < LAYER_ONE_LEN {
    leaves.push(random_output());
  }

  let output =
    leaves[usize::try_from(OsRng.next_u64() % u64::try_from(leaves.len()).unwrap()).unwrap()];

  let mut selene_hash = Some({
    let mut multiexp = vec![];
    for (scalar, point) in leaves
      .iter()
      .flat_map(|output| {
        let O = <Ed25519 as Ciphersuite>::G::to_xy(output.O).unwrap();
        let I = <Ed25519 as Ciphersuite>::G::to_xy(output.I).unwrap();
        let C = <Ed25519 as Ciphersuite>::G::to_xy(output.C).unwrap();
        [O.0, O.1, I.0, I.1, C.0, C.1]
      })
      .zip(params.curve_1_generators.g_bold_slice())
    {
      multiexp.push((scalar, *point));
    }
    params.curve_1_hash_init + multiexp_vartime(&multiexp)
  });
  let mut helios_hash = None;

  let mut curve_2_layers = vec![];
  let mut curve_1_layers = vec![];
  loop {
    if layers == 1 {
      break;
    }

    let mut curve_2_layer = vec![];
    while curve_2_layer.len() < LAYER_TWO_LEN {
      curve_2_layer.push(<Selene as Ciphersuite>::G::random(&mut OsRng));
    }
    let layer_len = curve_2_layer.len();
    curve_2_layer[usize::try_from(OsRng.next_u64()).unwrap() % layer_len] =
      selene_hash.take().unwrap();
    let curve_2_layer = curve_2_layer
      .into_iter()
      .map(|point| <Selene as Ciphersuite>::G::to_xy(point).unwrap().0)
      .collect::<Vec<_>>();

    helios_hash = Some({
      let mut multiexp = vec![];
      for (scalar, point) in curve_2_layer.iter().zip(params.curve_2_generators.g_bold_slice()) {
        multiexp.push((*scalar, *point));
      }
      params.curve_2_hash_init + multiexp_vartime(&multiexp)
    });

    curve_2_layers.push(curve_2_layer);

    if (1 + curve_1_layers.len() + curve_2_layers.len()) == layers {
      break;
    }

    let mut curve_1_layer = vec![];
    while curve_1_layer.len() < LAYER_ONE_LEN {
      curve_1_layer.push(<Helios as Ciphersuite>::G::random(&mut OsRng));
    }
    let layer_len = curve_1_layer.len();
    curve_1_layer[usize::try_from(OsRng.next_u64()).unwrap() % layer_len] =
      helios_hash.take().unwrap();
    let curve_1_layer = curve_1_layer
      .into_iter()
      .map(|point| <Helios as Ciphersuite>::G::to_xy(point).unwrap().0)
      .collect::<Vec<_>>();

    selene_hash = Some({
      let mut multiexp = vec![];
      for (scalar, point) in curve_1_layer.iter().zip(params.curve_1_generators.g_bold_slice()) {
        multiexp.push((*scalar, *point));
      }
      params.curve_1_hash_init + multiexp_vartime(&multiexp)
    });

    curve_1_layers.push(curve_1_layer);

    if (1 + curve_1_layers.len() + curve_2_layers.len()) == layers {
      break;
    }
  }

  let root = if let Some(selene_hash) = selene_hash {
    TreeRoot::<Selene, Helios>::C1(selene_hash)
  } else {
    TreeRoot::<Selene, Helios>::C2(helios_hash.unwrap())
  };

  (Path { output, leaves, curve_2_layers, curve_1_layers }, root)
}

fn random_paths(
  params: &FcmpParams<MoneroCurves>,
  layers: usize,
  paths: usize,
) -> (Vec<Path<MoneroCurves>>, TreeRoot<Selene, Helios>) {
  assert!(paths >= 1);
  assert!(paths <= LAYER_ONE_LEN.min(LAYER_TWO_LEN));

  let mut res = vec![];
  for _ in 0 .. paths {
    let (path, _root) = random_path(params, layers);
    res.push(path);
  }

  // Pop each path's top layer
  // Then push a new top layer which is unified for all paths
  // 1st layer has a C1 root (so the top layer is the leaves)
  // 2nd layer has a C2 root (so the top layer is C1)
  // 3rd layer has a C1 root (so the top layer is C2)
  let root = if layers == 1 {
    let mut outputs = vec![];
    for path in &res {
      outputs.push(path.output);
    }
    while outputs.len() < LAYER_ONE_LEN {
      outputs.push(random_output());
    }
    let mut shuffled_outputs = vec![];
    while !outputs.is_empty() {
      let i = usize::try_from(OsRng.next_u64() % u64::try_from(outputs.len()).unwrap()).unwrap();
      shuffled_outputs.push(outputs.swap_remove(i));
    }

    for path in &mut res {
      path.leaves = shuffled_outputs.clone();
    }

    let mut new_leaves_layer = vec![];
    for output in shuffled_outputs {
      let O = <Ed25519 as Ciphersuite>::G::to_xy(output.O).unwrap();
      let I = <Ed25519 as Ciphersuite>::G::to_xy(output.I).unwrap();
      let C = <Ed25519 as Ciphersuite>::G::to_xy(output.C).unwrap();
      new_leaves_layer.extend(&[O.0, O.1, I.0, I.1, C.0, C.1]);
    }

    TreeRoot::C1(
      hash_grow(
        &params.curve_1_generators,
        params.curve_1_hash_init,
        0,
        <Selene as Ciphersuite>::F::ZERO,
        &new_leaves_layer,
      )
      .unwrap(),
    )
  } else if (layers % 2) == 0 {
    let mut branch = vec![];
    for path in &res {
      branch.push(
        <Selene as Ciphersuite>::G::to_xy(if let Some(branch) = path.curve_1_layers.last() {
          hash_grow(
            &params.curve_1_generators,
            params.curve_1_hash_init,
            0,
            <Selene as Ciphersuite>::F::ZERO,
            branch,
          )
          .unwrap()
        } else {
          let mut leaves_layer = vec![];
          for output in &path.leaves {
            let O = <Ed25519 as Ciphersuite>::G::to_xy(output.O).unwrap();
            let I = <Ed25519 as Ciphersuite>::G::to_xy(output.I).unwrap();
            let C = <Ed25519 as Ciphersuite>::G::to_xy(output.C).unwrap();
            leaves_layer.extend(&[O.0, O.1, I.0, I.1, C.0, C.1]);
          }

          hash_grow(
            &params.curve_1_generators,
            params.curve_1_hash_init,
            0,
            <Selene as Ciphersuite>::F::ZERO,
            &leaves_layer,
          )
          .unwrap()
        })
        .unwrap()
        .0,
      );
    }
    while branch.len() < LAYER_TWO_LEN {
      branch.push(<Helios as Ciphersuite>::F::random(&mut OsRng));
    }
    let mut shuffled_branch = vec![];
    while !branch.is_empty() {
      let i = usize::try_from(OsRng.next_u64() % u64::try_from(branch.len()).unwrap()).unwrap();
      shuffled_branch.push(branch.swap_remove(i));
    }

    for path in &mut res {
      *path.curve_2_layers.last_mut().unwrap() = shuffled_branch.clone();
    }

    TreeRoot::C2(
      hash_grow(
        &params.curve_2_generators,
        params.curve_2_hash_init,
        0,
        <Helios as Ciphersuite>::F::ZERO,
        &shuffled_branch,
      )
      .unwrap(),
    )
  } else {
    let mut branch = vec![];
    for path in &res {
      branch.push(
        <Helios as Ciphersuite>::G::to_xy({
          let branch = path.curve_2_layers.last().unwrap();
          hash_grow(
            &params.curve_2_generators,
            params.curve_2_hash_init,
            0,
            <Helios as Ciphersuite>::F::ZERO,
            branch,
          )
          .unwrap()
        })
        .unwrap()
        .0,
      );
    }
    while branch.len() < LAYER_ONE_LEN {
      branch.push(<Selene as Ciphersuite>::F::random(&mut OsRng));
    }
    let mut shuffled_branch = vec![];
    while !branch.is_empty() {
      let i = usize::try_from(OsRng.next_u64() % u64::try_from(branch.len()).unwrap()).unwrap();
      shuffled_branch.push(branch.swap_remove(i));
    }

    for path in &mut res {
      *path.curve_1_layers.last_mut().unwrap() = shuffled_branch.clone();
    }

    TreeRoot::C1(
      hash_grow(
        &params.curve_1_generators,
        params.curve_1_hash_init,
        0,
        <Selene as Ciphersuite>::F::ZERO,
        &shuffled_branch,
      )
      .unwrap(),
    )
  };

  // Verify each of these paths are valid
  for path in &res {
    assert!(path.leaves.iter().any(|output| output == &path.output));

    let mut leaves_layer = vec![];
    for output in &path.leaves {
      let O = <Ed25519 as Ciphersuite>::G::to_xy(output.O).unwrap();
      let I = <Ed25519 as Ciphersuite>::G::to_xy(output.I).unwrap();
      let C = <Ed25519 as Ciphersuite>::G::to_xy(output.C).unwrap();
      leaves_layer.extend(&[O.0, O.1, I.0, I.1, C.0, C.1]);
    }

    let mut c1_hash = Some(
      <Selene as Ciphersuite>::G::to_xy(
        hash_grow(
          &params.curve_1_generators,
          params.curve_1_hash_init,
          0,
          <Selene as Ciphersuite>::F::ZERO,
          &leaves_layer,
        )
        .unwrap(),
      )
      .unwrap()
      .0,
    );
    let mut c2_hash = None;

    let mut c1s = path.curve_1_layers.iter();
    let mut c2s = path.curve_2_layers.iter();
    loop {
      if let Some(layer) = c2s.next() {
        assert!(layer.iter().any(|leaf| leaf == &c1_hash.unwrap()));
        c1_hash = None;
        c2_hash = Some(
          <Helios as Ciphersuite>::G::to_xy(
            hash_grow(
              &params.curve_2_generators,
              params.curve_2_hash_init,
              0,
              <Helios as Ciphersuite>::F::ZERO,
              layer,
            )
            .unwrap(),
          )
          .unwrap()
          .0,
        );
      } else {
        assert!(c1s.next().is_none());
      }

      if let Some(layer) = c1s.next() {
        assert!(layer.iter().any(|leaf| leaf == &c2_hash.unwrap()));
        c2_hash = None;
        c1_hash = Some(
          <Selene as Ciphersuite>::G::to_xy(
            hash_grow(
              &params.curve_1_generators,
              params.curve_1_hash_init,
              0,
              <Selene as Ciphersuite>::F::ZERO,
              layer,
            )
            .unwrap(),
          )
          .unwrap()
          .0,
        );
      } else {
        assert!(c2s.next().is_none());
        break;
      }
    }
    match root {
      TreeRoot::C1(root) => {
        assert_eq!(<Selene as Ciphersuite>::G::to_xy(root).unwrap().0, c1_hash.unwrap());
      }
      TreeRoot::C2(root) => {
        assert_eq!(<Helios as Ciphersuite>::G::to_xy(root).unwrap().0, c2_hash.unwrap());
      }
    }
  }

  (res, root)
}

fn random_output_blinds(
  G: <Ed25519 as Ciphersuite>::G,
  T: <Ed25519 as Ciphersuite>::G,
  U: <Ed25519 as Ciphersuite>::G,
  V: <Ed25519 as Ciphersuite>::G,
) -> OutputBlinds<<Ed25519 as Ciphersuite>::G> {
  let output_blinds_start = std::time::Instant::now();
  let res = OutputBlinds::new(
    OBlind::new(
      T,
      ScalarDecomposition::new(<Ed25519 as Ciphersuite>::F::random(&mut OsRng)).unwrap(),
    ),
    IBlind::new(
      U,
      V,
      ScalarDecomposition::new(<Ed25519 as Ciphersuite>::F::random(&mut OsRng)).unwrap(),
    ),
    IBlindBlind::new(
      T,
      ScalarDecomposition::new(<Ed25519 as Ciphersuite>::F::random(&mut OsRng)).unwrap(),
    ),
    CBlind::new(
      G,
      ScalarDecomposition::new(<Ed25519 as Ciphersuite>::F::random(&mut OsRng)).unwrap(),
    ),
  );
  println!("Output blinds took {}ms to calculate", output_blinds_start.elapsed().as_millis());
  res
}

fn blind_branches(
  params: &FcmpParams<MoneroCurves>,
  branches: Branches<MoneroCurves>,
  output_blinds: Vec<OutputBlinds<<Ed25519 as Ciphersuite>::G>>,
) -> BranchesWithBlinds<MoneroCurves> {
  let branch_blinds_start = std::time::Instant::now();
  let mut branches_1_blinds = vec![];
  for _ in 0 .. branches.necessary_c1_blinds() {
    branches_1_blinds.push(BranchBlind::<<Selene as Ciphersuite>::G>::new(
      params.curve_1_generators.h(),
      ScalarDecomposition::new(<Selene as Ciphersuite>::F::random(&mut OsRng)).unwrap(),
    ));
  }

  let mut branches_2_blinds = vec![];
  for _ in 0 .. branches.necessary_c2_blinds() {
    branches_2_blinds.push(BranchBlind::<<Helios as Ciphersuite>::G>::new(
      params.curve_2_generators.h(),
      ScalarDecomposition::new(<Helios as Ciphersuite>::F::random(&mut OsRng)).unwrap(),
    ));
  }
  println!(
    "{} C1 branch blinds and {} C2 branch blinds took {}ms to calculate",
    branches.necessary_c1_blinds(),
    branches.necessary_c2_blinds(),
    branch_blinds_start.elapsed().as_millis()
  );

  branches.blind(output_blinds, branches_1_blinds, branches_2_blinds).unwrap()
}

#[inline(never)]
fn verify_fn(
  iters: usize,
  batch: usize,
  proof: &Fcmp<MoneroCurves>,
  params: &FcmpParams<MoneroCurves>,
  root: TreeRoot<Selene, Helios>,
  layers: usize,
  inputs: &[Input<<Selene as Ciphersuite>::F>],
) {
  let mut times = vec![];
  for _ in 0 .. iters {
    let instant = std::time::Instant::now();

    let mut verifier_1 = generalized_bulletproofs::Generators::batch_verifier();
    let mut verifier_2 = generalized_bulletproofs::Generators::batch_verifier();

    for _ in 0 .. batch {
      proof
        .verify(&mut OsRng, &mut verifier_1, &mut verifier_2, params, root, layers, inputs)
        .unwrap();
    }

    assert!(params.curve_1_generators.verify(verifier_1));
    assert!(params.curve_2_generators.verify(verifier_2));

    times.push(instant.elapsed().as_millis());
  }
  times.sort_unstable();
  println!("Median time to verify {batch} proof(s) was {}ms (n={iters})", times[times.len() / 2]);
}

#[test]
fn test_single_input() {
  let (G, T, U, V, params) = random_params(1);

  let output_blinds = random_output_blinds(G, T, U, V);

  for layers in 1 ..= (TARGET_LAYERS + 1) {
    println!("Testing a proof with 1 input and {layers} layers");

    let (path, root) = random_path(&params, layers);
    let output = path.output;

    let branches = Branches::new(vec![path]).unwrap();

    let input = output_blinds.blind(&output).unwrap();

    let proof = Fcmp::prove(
      &mut OsRng,
      &params,
      blind_branches(&params, branches, vec![output_blinds.clone()]),
    )
    .unwrap();

    verify_fn(1, 1, &proof, &params, root, layers, &[input]);
  }
}

#[test]
fn test_multiple_inputs() {
  let (G, T, U, V, params) = random_params(8);

  let mut all_proofs = vec![];

  for paths in 2 ..= 4 {
    // This is less than target layers yet still tests a C1 root and a C2 root
    for layers in 1 ..= 4 {
      println!("Testing a proof with {paths} inputs and {layers} layers");

      let (paths, root) = random_paths(&params, layers, paths);

      let mut output_blinds = vec![];
      for _ in 0 .. paths.len() {
        output_blinds.push(random_output_blinds(G, T, U, V));
      }

      let mut inputs = vec![];
      for (path, output_blinds) in paths.iter().zip(&output_blinds) {
        inputs.push(output_blinds.blind(&path.output).unwrap());
      }

      let branches = Branches::new(paths).unwrap();

      let proof =
        Fcmp::prove(&mut OsRng, &params, blind_branches(&params, branches, output_blinds)).unwrap();
      all_proofs.push((root, layers, inputs.clone(), proof.clone()));

      verify_fn(1, 1, &proof, &params, root, layers, &inputs);
    }
  }

  // Test batch verification of all of these proofs
  let mut verifier_1 = generalized_bulletproofs::Generators::batch_verifier();
  let mut verifier_2 = generalized_bulletproofs::Generators::batch_verifier();

  for (root, layers, inputs, proof) in all_proofs {
    proof
      .verify(&mut OsRng, &mut verifier_1, &mut verifier_2, &params, root, layers, &inputs)
      .unwrap();
  }
  assert!(params.curve_1_generators.verify(verifier_1));
  assert!(params.curve_2_generators.verify(verifier_2));
}

#[test]
fn test_malleated_proofs() {
  let (G, T, U, V, params) = random_params(2);

  for paths in [1, 2] {
    for layers in [1, 2] {
      let (paths, root) = random_paths(&params, layers, paths);

      let mut output_blinds = vec![];
      for _ in 0 .. paths.len() {
        output_blinds.push(random_output_blinds(G, T, U, V));
      }

      let mut inputs = vec![];
      for (path, output_blinds) in paths.iter().zip(&output_blinds) {
        inputs.push(output_blinds.blind(&path.output).unwrap());
      }

      let branches = Branches::new(paths.clone()).unwrap();

      let mut buf = vec![];
      {
        let proof =
          Fcmp::prove(&mut OsRng, &params, blind_branches(&params, branches, output_blinds))
            .unwrap();
        proof.write(&mut buf).unwrap();
      }

      for i in 0 .. buf.len() {
        let mut buf = buf.clone();
        let existing_byte = buf[i];
        while buf[i] == existing_byte {
          buf[i] = u8::try_from(OsRng.next_u64() & u64::from(u8::MAX)).unwrap();
        }
        if let Ok(proof) = Fcmp::read(&mut buf.as_slice(), paths.len(), layers) {
          let mut verifier_1 = generalized_bulletproofs::Generators::batch_verifier();
          let mut verifier_2 = generalized_bulletproofs::Generators::batch_verifier();

          if proof
            .verify(&mut OsRng, &mut verifier_1, &mut verifier_2, &params, root, layers, &inputs)
            .is_ok()
          {
            let valid = params.curve_1_generators.verify(verifier_1) &&
              params.curve_2_generators.verify(verifier_2);
            assert!(!valid, "malleated proof yet still verified");
          }
        }
      }
    }
  }
}

#[test]
fn prove_benchmark() {
  const RUNS: usize = 10;

  let (G, T, U, V, params) = random_params(8);

  for paths in 1 ..= 4 {
    let (paths, _root) = random_paths(&params, TARGET_LAYERS, paths);

    let mut set_size = 1u64;
    for i in 0 .. TARGET_LAYERS {
      if i % 2 == 0 {
        set_size *= u64::try_from(LAYER_ONE_LEN).unwrap();
      } else {
        set_size *= u64::try_from(LAYER_TWO_LEN).unwrap();
      }
    }

    let branches = Branches::new(paths.clone()).unwrap();

    let prove_start = std::time::Instant::now();
    for _ in 0 .. 10 {
      let mut output_blinds = vec![];
      for _ in 0 .. paths.len() {
        output_blinds.push(random_output_blinds(G, T, U, V));
      }

      let proof =
        Fcmp::prove(&mut OsRng, &params, blind_branches(&params, branches.clone(), output_blinds))
          .unwrap();

      core::hint::black_box(proof);
    }
    #[allow(clippy::print_literal)]
    {
      println!(
        "{} for {RUNS} {}-input FCMPs with a set size of {} took an average of {}ms each",
        "Sequentially proving",
        paths.len(),
        set_size,
        prove_start.elapsed().as_millis() / u128::try_from(RUNS).unwrap()
      );
    }
  }
}

#[test]
fn verify_benchmark() {
  let (G, T, U, V, params) = random_params(1);

  let (path, root) = random_path(&params, TARGET_LAYERS);
  let output = path.output;

  let branches = Branches::new(vec![path]).unwrap();

  let output_blinds = random_output_blinds(G, T, U, V);
  let input = output_blinds.blind(&output).unwrap();

  let proof =
    Fcmp::prove(&mut OsRng, &params, blind_branches(&params, branches, vec![output_blinds]))
      .unwrap();

  verify_fn(100, 1, &proof, &params, root, TARGET_LAYERS, &[input]);
  verify_fn(100, 10, &proof, &params, root, TARGET_LAYERS, &[input]);
  verify_fn(100, 100, &proof, &params, root, TARGET_LAYERS, &[input]);
}

#[test]
fn proof_sizes() {
  for inputs in 0 ..= 256 {
    println!(
      "Proof size for {inputs} inputs: {}",
      Fcmp::<MoneroCurves>::proof_size(inputs, TARGET_LAYERS)
    );
  }
}

// Tests for verifying with the constraints folded as they're produced (`Fcmp::verify`) against
// verifying from the stored constraints (`Fcmp::verify_stored`, the historical flow, whose
// `ArithmeticCircuitStatement::verify` is unchanged).

/// A deterministic RNG, so two verifications sample the same batch-verifier weights.
///
/// This is solely for these tests and is not secure for any other purpose.
#[derive(Clone)]
struct DeterministicRng {
  seed: [u8; 32],
  counter: u64,
}
impl DeterministicRng {
  fn new() -> Self {
    let mut seed = [0; 32];
    OsRng.fill_bytes(&mut seed);
    Self { seed, counter: 0 }
  }
}
impl RngCore for DeterministicRng {
  fn next_u32(&mut self) -> u32 {
    rand_core::impls::next_u32_via_fill(self)
  }
  fn next_u64(&mut self) -> u64 {
    rand_core::impls::next_u64_via_fill(self)
  }
  fn fill_bytes(&mut self, dest: &mut [u8]) {
    use blake2::{Digest as _, Blake2b512};
    for chunk in dest.chunks_mut(64) {
      let block = Blake2b512::new()
        .chain_update(self.seed)
        .chain_update(self.counter.to_le_bytes())
        .finalize();
      self.counter += 1;
      chunk.copy_from_slice(&block[.. chunk.len()]);
    }
  }
  fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand_core::Error> {
    self.fill_bytes(dest);
    Ok(())
  }
}
impl CryptoRng for DeterministicRng {}

fn assert_verifiers_eq<C: Ciphersuite>(
  a: &generalized_bulletproofs::BatchVerifier<C>,
  b: &generalized_bulletproofs::BatchVerifier<C>,
) {
  assert_eq!(a.g, b.g);
  assert_eq!(a.h, b.h);
  assert_eq!(a.g_bold, b.g_bold);
  assert_eq!(a.h_bold, b.h_bold);
  assert_eq!(a.h_sum, b.h_sum);
  // As these circuits don't use Pedersen commitments, the order is identical as well
  assert_eq!(a.additional, b.additional);
}

#[allow(clippy::type_complexity)]
fn random_proof(
  params: &FcmpParams<MoneroCurves>,
  (G, T, U, V): (
    <Ed25519 as Ciphersuite>::G,
    <Ed25519 as Ciphersuite>::G,
    <Ed25519 as Ciphersuite>::G,
    <Ed25519 as Ciphersuite>::G,
  ),
  paths: usize,
  layers: usize,
) -> (Fcmp<MoneroCurves>, TreeRoot<Selene, Helios>, Vec<Input<<Selene as Ciphersuite>::F>>) {
  let (paths, root) = random_paths(params, layers, paths);

  let mut output_blinds = vec![];
  for _ in 0 .. paths.len() {
    output_blinds.push(random_output_blinds(G, T, U, V));
  }

  let mut inputs = vec![];
  for (path, output_blinds) in paths.iter().zip(&output_blinds) {
    inputs.push(output_blinds.blind(&path.output).unwrap());
  }

  let branches = Branches::new(paths).unwrap();
  let proof =
    Fcmp::prove(&mut OsRng, params, blind_branches(params, branches, output_blinds)).unwrap();
  (proof, root, inputs)
}

/// The result of a verification, and the batch verifiers after it.
type VerificationResult = (
  Result<(), FcmpError>,
  generalized_bulletproofs::BatchVerifier<Selene>,
  generalized_bulletproofs::BatchVerifier<Helios>,
);

/// Verify a proof with the constraints folded (`Fcmp::verify`) and stored (`Fcmp::verify_stored`),
/// using the same randomness.
fn verify_both(
  proof: &Fcmp<MoneroCurves>,
  params: &FcmpParams<MoneroCurves>,
  root: TreeRoot<Selene, Helios>,
  layers: usize,
  inputs: &[Input<<Selene as Ciphersuite>::F>],
) -> (VerificationResult, VerificationResult) {
  let rng = DeterministicRng::new();

  let mut folded_1 = generalized_bulletproofs::Generators::batch_verifier();
  let mut folded_2 = generalized_bulletproofs::Generators::batch_verifier();
  let folded =
    proof.verify(&mut rng.clone(), &mut folded_1, &mut folded_2, params, root, layers, inputs);

  let mut stored_1 = generalized_bulletproofs::Generators::batch_verifier();
  let mut stored_2 = generalized_bulletproofs::Generators::batch_verifier();
  let stored = proof.verify_stored(
    &mut rng.clone(),
    &mut stored_1,
    &mut stored_2,
    params,
    root,
    layers,
    inputs,
  );

  ((folded, folded_1, folded_2), (stored, stored_1, stored_2))
}

fn batch_verifies(
  params: &FcmpParams<MoneroCurves>,
  verifier_1: &generalized_bulletproofs::BatchVerifier<Selene>,
  verifier_2: &generalized_bulletproofs::BatchVerifier<Helios>,
) -> bool {
  params.curve_1_generators.verify(verifier_1.clone()) &&
    params.curve_2_generators.verify(verifier_2.clone())
}

#[test]
fn test_folded_verify_matches_stored() {
  let (G, T, U, V, params) = random_params(4);

  // Both C1 and C2 roots, with and without branches on either curve
  for (paths, layers) in [(1, 1), (1, 2), (1, 3), (1, 5), (2, 1), (2, 4), (3, 2), (3, 3)] {
    println!("Comparing verifiers for a proof with {paths} inputs and {layers} layers");
    let (proof, root, inputs) = random_proof(&params, (G, T, U, V), paths, layers);

    let ((folded, folded_1, folded_2), (stored, stored_1, stored_2)) =
      verify_both(&proof, &params, root, layers, &inputs);
    folded.unwrap();
    stored.unwrap();
    assert_verifiers_eq(&folded_1, &stored_1);
    assert_verifiers_eq(&folded_2, &stored_2);
    assert!(batch_verifies(&params, &folded_1, &folded_2));
    assert!(batch_verifies(&params, &stored_1, &stored_2));
  }
}

#[test]
fn test_folded_verify_matches_stored_on_tampered_proofs() {
  // The test generators must be a power of two in count: `random_params(4)`
  let (G, T, U, V, params) = random_params(4);
  let mut rng = DeterministicRng::new();

  // Both C1 and C2 roots, with branches on both curves
  for (paths, layers) in [(1, 1), (1, 2), (2, 3), (3, 2)] {
    let (proof, root, inputs) = random_proof(&params, (G, T, U, V), paths, layers);
    let mut buf = vec![];
    proof.write(&mut buf).unwrap();

    // Single-bit flips at random positions throughout the serialized proof
    let mut cases = vec![];
    for _ in 0 .. 64 {
      let offset = usize::try_from(rng.next_u64() % u64::try_from(buf.len()).unwrap()).unwrap();
      let bit = rng.next_u32() % 8;
      let mut flipped = buf.clone();
      flipped[offset] ^= 1 << bit;
      cases.push((flipped, inputs.clone(), offset, bit));
    }
    // The genuine proof, for a different statement (the inputs in a different order)
    if paths >= 2 {
      let mut swapped = inputs.clone();
      swapped.swap(0, 1);
      cases.push((buf.clone(), swapped, usize::MAX, 0));
    }

    for (bytes, inputs, offset, bit) in cases {
      let tampered = Fcmp::<MoneroCurves>::read(&mut bytes.as_slice(), paths, layers).unwrap();
      let ((folded, folded_1, folded_2), (stored, stored_1, stored_2)) =
        verify_both(&tampered, &params, root, layers, &inputs);
      // The same transcript operations are performed, so the same proofs are well-formed
      assert_eq!(folded.is_ok(), stored.is_ok(), "{paths} {layers} {offset} {bit}");
      if folded.is_ok() {
        assert_verifiers_eq(&folded_1, &stored_1);
        assert_verifiers_eq(&folded_2, &stored_2);
        assert!(!batch_verifies(&params, &folded_1, &folded_2), "{paths} {layers} {offset} {bit}");
      }
    }
  }
}

#[test]
fn test_discarded_and_folded_circuit_statements() {
  use generalized_bulletproofs::transcript::VerifierTranscript;
  use generalized_bulletproofs_circuit_abstraction::{
    Circuit as UnderlyingCircuit, CircuitStatementError,
  };

  let (_, _, _, _, params) = random_params(1);

  // A circuit which discarded its constraints can't yield a statement
  let no_commitments =
    VerifierTranscript::new([0; 32], &[]).read_commitments::<Selene>(0, 0).unwrap();
  assert_eq!(
    UnderlyingCircuit::<Selene>::verify_discarding()
      .statement(params.curve_1_generators.reduce(1).unwrap(), no_commitments)
      .err(),
    Some(CircuitStatementError::ConstraintsDiscarded),
  );

  // A circuit which folds its constraints refuses Pedersen commitments
  let point = <Selene as Ciphersuite>::G::random(&mut OsRng).to_bytes();
  let one_commitment =
    VerifierTranscript::new([0; 32], point.as_ref()).read_commitments::<Selene>(0, 1).unwrap();
  let z = <Selene as Ciphersuite>::F::random(&mut OsRng);
  assert_eq!(
    UnderlyingCircuit::<Selene>::verify_folding(z)
      .statement(params.curve_1_generators.reduce(1).unwrap(), one_commitment)
      .err(),
    Some(CircuitStatementError::FoldedWithPedersenCommitments),
  );
}

// A measurement, not a check: the peak heap of one verification with the constraints stored (the
// historical flow) and folded, per proof size. Run alone: `-- --test-threads=1 --nocapture`.
mod verify_memory {
  use core::sync::atomic::{AtomicUsize, Ordering::Relaxed};
  use std::alloc::{GlobalAlloc, Layout, System};

  pub(super) static ALLOCATED: AtomicUsize = AtomicUsize::new(0);
  pub(super) static PEAK: AtomicUsize = AtomicUsize::new(0);

  struct CountingAllocator;
  unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
      let ptr = unsafe { System.alloc(layout) };
      if !ptr.is_null() {
        let now = ALLOCATED.fetch_add(layout.size(), Relaxed) + layout.size();
        PEAK.fetch_max(now, Relaxed);
      }
      ptr
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
      unsafe { System.dealloc(ptr, layout) };
      ALLOCATED.fetch_sub(layout.size(), Relaxed);
    }
  }
  #[global_allocator]
  static GLOBAL: CountingAllocator = CountingAllocator;
}

#[test]
fn verify_memory_benchmark() {
  use core::sync::atomic::Ordering::Relaxed;
  use verify_memory::{ALLOCATED, PEAK};

  // `random_paths` can produce at most `LAYER_TWO_LEN` (18) paths, so this stops at 16 inputs; the
  // node-level measurements cover the 128-input proofs.
  for inputs in [1, 2, 8, 16] {
    let (G, T, U, V, params) = random_params(inputs);
    let (proof, root, input_vec) = random_proof(&params, (G, T, U, V), inputs, TARGET_LAYERS);
    let rng = DeterministicRng::new();

    for (name, stored) in [("stored", true), ("folded", false)] {
      let mut verifier_1 = generalized_bulletproofs::Generators::batch_verifier();
      let mut verifier_2 = generalized_bulletproofs::Generators::batch_verifier();
      let before = ALLOCATED.load(Relaxed);
      PEAK.store(before, Relaxed);
      let start = std::time::Instant::now();
      let res = if stored {
        proof.verify_stored(
          &mut rng.clone(),
          &mut verifier_1,
          &mut verifier_2,
          &params,
          root,
          TARGET_LAYERS,
          &input_vec,
        )
      } else {
        proof.verify(
          &mut rng.clone(),
          &mut verifier_1,
          &mut verifier_2,
          &params,
          root,
          TARGET_LAYERS,
          &input_vec,
        )
      };
      let elapsed = start.elapsed().as_millis();
      let peak = PEAK.load(Relaxed) - before;
      res.unwrap();
      assert!(batch_verifies(&params, &verifier_1, &verifier_2));
      println!(
        "VERIFY_MEMORY inputs={inputs} layers={TARGET_LAYERS} {name}: peak_heap_bytes={peak} ms={elapsed}"
      );
    }
  }
}
