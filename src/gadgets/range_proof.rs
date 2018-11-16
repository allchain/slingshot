use bulletproofs::r1cs::{ConstraintSystem, Variable, R1CSError};
use curve25519_dalek::scalar::Scalar;
use error::SpacesuitError;
use value::AllocatedQuantity;

/// Enforces that the quantity of v is in the range [0, 2^n).
pub fn fill_cs<CS: ConstraintSystem>(
    cs: &mut CS,
    v: AllocatedQuantity,
    n: usize,
) -> Result<(), SpacesuitError> {
    let one = Scalar::one();
    let one_var = Variable::One();

    let mut constraint = vec![(v.variable, -one)];
    let mut exp_2 = Scalar::one();
    for i in 0..n {
        // Create low-level variables and add them to constraints
        let (a, b, o) = cs.allocate(||{
            let q: u64 = v.assignment.ok_or(R1CSError::MissingAssignment)?;
            let bit: u64 = (q >> i) & 1;
            Ok(((1 - bit).into(), bit.into(), Scalar::zero()))
        })?;

        // Enforce a * b = 0, so one of (a,b) is zero
        cs.constrain(o.into());

        // Enforce that a = 1 - b, so they both are 1 or 0.
        cs.constrain(a + (b - 1));

        constraint.push((b, exp_2));
        exp_2 = exp_2 + exp_2;
    }

    // Enforce that v = Sum(b_i * 2^i, i = 0..n-1)
    cs.constrain(constraint.iter().collect());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bulletproofs::r1cs::{ProverCS, VerifierCS};
    use bulletproofs::{BulletproofGens, PedersenGens};
    use merlin::Transcript;

    #[test]
    fn range_proof_gadget() {
        use rand::rngs::OsRng;
        use rand::Rng;

        let mut rng = OsRng::new().unwrap();
        let m = 3; // number of values to test per `n`

        for n in [2, 10, 32, 63].iter() {
            let (min, max) = (0u64, ((1u128 << n) - 1) as u64);
            let values: Vec<u64> = (0..m).map(|_| rng.gen_range(min, max)).collect();
            for v in values {
                assert!(range_proof_helper(v, *n).is_ok());
            }
            assert!(range_proof_helper(max + 1, *n).is_err());
        }
    }

    fn range_proof_helper(v_val: u64, n: usize) -> Result<(), SpacesuitError> {
        // Common
        let pc_gens = PedersenGens::default();
        let bp_gens = BulletproofGens::new(128, 1);

        // Prover's scope
        let (proof, commitments) = {
            // Prover makes a `ConstraintSystem` instance representing a merge gadget
            // v and v_blinding emptpy because we are only testing low-level variable constraints
            let v = vec![];
            let v_blinding = vec![];
            let mut prover_transcript = Transcript::new(b"RangeProofTest");
            let (mut prover_cs, _variables, commitments) = ProverCS::new(
                &bp_gens,
                &pc_gens,
                &mut prover_transcript,
                v,
                v_blinding.clone(),
            );

            // Prover allocates variables and adds constraints to the constraint system
            let (v_var, _) =
                prover_cs.assign_uncommitted(Assignment::from(v_val), Scalar::zero().into())?;

            fill_cs(&mut prover_cs, (v_var, Assignment::from(v_val)), n)?;

            let proof = prover_cs.prove()?;

            (proof, commitments)
        };

        // Verifier makes a `ConstraintSystem` instance representing a merge gadget
        let mut verifier_transcript = Transcript::new(b"RangeProofTest");
        let (mut verifier_cs, _variables) =
            VerifierCS::new(&bp_gens, &pc_gens, &mut verifier_transcript, commitments);

        // Verifier allocates variables and adds constraints to the constraint system
        let (v_var, _) =
            verifier_cs.assign_uncommitted(Assignment::Missing(), Assignment::Missing())?;

        assert!(fill_cs(&mut verifier_cs, (v_var, Assignment::Missing()), n).is_ok());

        Ok(verifier_cs.verify(&proof)?)
    }
}
