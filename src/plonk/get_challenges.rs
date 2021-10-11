use std::collections::HashSet;

use crate::field::extension_field::Extendable;
use crate::field::field_types::{Field, RichField};
use crate::fri::verifier::{compute_evaluation, fri_combine_initial, PrecomputedReducedEvals};
use crate::hash::hashing::hash_n_to_1;
use crate::iop::challenger::Challenger;
use crate::plonk::circuit_data::CommonCircuitData;
use crate::plonk::proof::{
    CompressedProofWithPublicInputs, ProofChallenges, ProofWithPublicInputs,
};
use crate::util::reverse_bits;

impl<F: RichField + Extendable<D>, const D: usize> ProofWithPublicInputs<F, D> {
    pub(crate) fn fri_query_indices(
        &self,
        common_data: &CommonCircuitData<F, D>,
    ) -> anyhow::Result<Vec<usize>> {
        Ok(self.get_challenges(common_data)?.fri_query_indices)
    }

    pub(crate) fn get_challenges(
        &self,
        common_data: &CommonCircuitData<F, D>,
    ) -> anyhow::Result<ProofChallenges<F, D>> {
        let config = &common_data.config;
        let num_challenges = config.num_challenges;
        let num_fri_queries = config.fri_config.num_query_rounds;
        let lde_size = common_data.lde_size();

        let mut challenger = Challenger::new();

        // Observe the instance.
        challenger.observe_hash(&common_data.circuit_digest);
        challenger.observe_hash(&self.get_public_inputs_hash());

        challenger.observe_cap(&self.proof.wires_cap);
        let plonk_betas = challenger.get_n_challenges(num_challenges);
        let plonk_gammas = challenger.get_n_challenges(num_challenges);

        challenger.observe_cap(&self.proof.plonk_zs_partial_products_cap);
        let plonk_alphas = challenger.get_n_challenges(num_challenges);

        challenger.observe_cap(&self.proof.quotient_polys_cap);
        let plonk_zeta = challenger.get_extension_challenge();

        challenger.observe_opening_set(&self.proof.openings);

        // Scaling factor to combine polynomials.
        let fri_alpha = challenger.get_extension_challenge();

        // Recover the random betas used in the FRI reductions.
        let fri_betas = self
            .proof
            .opening_proof
            .commit_phase_merkle_caps
            .iter()
            .map(|cap| {
                challenger.observe_cap(cap);
                challenger.get_extension_challenge()
            })
            .collect();

        challenger.observe_extension_elements(&self.proof.opening_proof.final_poly.coeffs);

        let fri_pow_response = hash_n_to_1(
            challenger
                .get_hash()
                .elements
                .iter()
                .copied()
                .chain(Some(self.proof.opening_proof.pow_witness))
                .collect(),
            false,
        );

        let fri_query_indices = (0..num_fri_queries)
            .map(|_| challenger.get_challenge().to_canonical_u64() as usize % lde_size)
            .collect();

        Ok(ProofChallenges {
            plonk_betas,
            plonk_gammas,
            plonk_alphas,
            plonk_zeta,
            fri_alpha,
            fri_betas,
            fri_pow_response,
            fri_query_indices,
            fri_query_inferred_elements: None,
        })
    }
}

impl<F: RichField + Extendable<D>, const D: usize> CompressedProofWithPublicInputs<F, D> {
    pub(crate) fn fri_query_indices(
        &self,
        common_data: &CommonCircuitData<F, D>,
    ) -> anyhow::Result<Vec<usize>> {
        Ok(self.get_challenges(common_data)?.fri_query_indices)
    }

    pub(crate) fn get_challenges(
        &self,
        common_data: &CommonCircuitData<F, D>,
    ) -> anyhow::Result<ProofChallenges<F, D>> {
        let config = &common_data.config;
        let num_challenges = config.num_challenges;
        let num_fri_queries = config.fri_config.num_query_rounds;
        let lde_size = common_data.lde_size();

        let mut challenger = Challenger::new();

        // Observe the instance.
        challenger.observe_hash(&common_data.circuit_digest);
        challenger.observe_hash(&self.get_public_inputs_hash());

        challenger.observe_cap(&self.proof.wires_cap);
        let plonk_betas = challenger.get_n_challenges(num_challenges);
        let plonk_gammas = challenger.get_n_challenges(num_challenges);

        challenger.observe_cap(&self.proof.plonk_zs_partial_products_cap);
        let plonk_alphas = challenger.get_n_challenges(num_challenges);

        challenger.observe_cap(&self.proof.quotient_polys_cap);
        let plonk_zeta = challenger.get_extension_challenge();

        challenger.observe_opening_set(&self.proof.openings);

        // Scaling factor to combine polynomials.
        let fri_alpha = challenger.get_extension_challenge();

        // Recover the random betas used in the FRI reductions.
        let fri_betas = self
            .proof
            .opening_proof
            .commit_phase_merkle_caps
            .iter()
            .map(|cap| {
                challenger.observe_cap(cap);
                challenger.get_extension_challenge()
            })
            .collect::<Vec<_>>();

        challenger.observe_extension_elements(&self.proof.opening_proof.final_poly.coeffs);

        let fri_pow_response = hash_n_to_1(
            challenger
                .get_hash()
                .elements
                .iter()
                .copied()
                .chain(Some(self.proof.opening_proof.pow_witness))
                .collect(),
            false,
        );

        let fri_query_indices = (0..num_fri_queries)
            .map(|_| challenger.get_challenge().to_canonical_u64() as usize % lde_size)
            .collect::<Vec<_>>();

        let mut seen_indices_by_depth =
            vec![HashSet::new(); common_data.fri_params.reduction_arity_bits.len()];
        let precomputed_reduced_evals =
            PrecomputedReducedEvals::from_os_and_alpha(&self.proof.openings, fri_alpha);
        let mut fri_query_inferred_elements = Vec::new();
        let log_n = common_data.degree_bits + common_data.config.rate_bits;
        for query_round in 0..common_data.config.fri_config.num_query_rounds {
            let mut x_index = fri_query_indices[query_round];
            let mut subgroup_x = F::MULTIPLICATIVE_GROUP_GENERATOR
                * F::primitive_root_of_unity(log_n).exp_u64(reverse_bits(x_index, log_n) as u64);
            let mut old_eval = fri_combine_initial(
                &self
                    .proof
                    .opening_proof
                    .query_round_proofs
                    .initial_trees_proofs[&x_index],
                fri_alpha,
                plonk_zeta,
                subgroup_x,
                precomputed_reduced_evals,
                common_data,
            );
            for (i, &arity_bits) in common_data
                .fri_params
                .reduction_arity_bits
                .iter()
                .enumerate()
            {
                if !seen_indices_by_depth[i].insert(x_index >> arity_bits) {
                    break;
                }
                fri_query_inferred_elements.push(old_eval);
                let arity = 1 << arity_bits;
                let mut evals = self.proof.opening_proof.query_round_proofs.steps[i]
                    [&(x_index >> arity_bits)]
                    .evals
                    .clone();
                let x_index_within_coset = x_index & (arity - 1);
                evals.insert(x_index_within_coset, old_eval);
                old_eval = compute_evaluation(
                    subgroup_x,
                    x_index_within_coset,
                    arity_bits,
                    &evals,
                    fri_betas[i],
                );
                subgroup_x = subgroup_x.exp_power_of_2(arity_bits);
                x_index >>= arity_bits;
            }
        }

        Ok(ProofChallenges {
            plonk_betas,
            plonk_gammas,
            plonk_alphas,
            plonk_zeta,
            fri_alpha,
            fri_betas,
            fri_pow_response,
            fri_query_indices,
            fri_query_inferred_elements: Some(fri_query_inferred_elements),
        })
    }
}
