use plonky2::field::extension_field::Extendable;
use plonky2::hash::hash_types::RichField;

use crate::config::StarkConfig;
use crate::cpu::cpu_stark::CpuStark;
use crate::cross_table_lookup::CrossTableLookup;
use crate::keccak::keccak_stark::KeccakStark;
use crate::stark::Stark;

#[derive(Clone)]
pub struct AllStark<F: RichField + Extendable<D>, const D: usize> {
    pub cpu_stark: CpuStark<F, D>,
    pub keccak_stark: KeccakStark<F, D>,
    pub cross_table_lookups: Vec<CrossTableLookup<F>>,
}

impl<F: RichField + Extendable<D>, const D: usize> AllStark<F, D> {
    pub(crate) fn nums_permutation_zs(&self, config: &StarkConfig) -> Vec<usize> {
        let ans = vec![
            self.cpu_stark.num_permutation_batches(config),
            self.keccak_stark.num_permutation_batches(config),
        ];
        debug_assert_eq!(ans.len(), Table::num_tables());
        ans
    }

    pub(crate) fn permutation_batch_sizes(&self) -> Vec<usize> {
        let ans = vec![
            self.cpu_stark.permutation_batch_size(),
            self.keccak_stark.permutation_batch_size(),
        ];
        debug_assert_eq!(ans.len(), Table::num_tables());
        ans
    }
}

#[derive(Copy, Clone)]
pub enum Table {
    Cpu = 0,
    Keccak = 1,
}

impl Table {
    pub(crate) fn num_tables() -> usize {
        Table::Keccak as usize + 1
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use itertools::Itertools;
    use plonky2::field::field_types::Field;
    use plonky2::field::polynomial::PolynomialValues;
    use plonky2::plonk::config::{GenericConfig, PoseidonGoldilocksConfig};
    use plonky2::util::timing::TimingTree;
    use rand::{thread_rng, Rng, SeedableRng};
    use rand_chacha::ChaCha8Rng;

    use crate::all_stark::{AllStark, Table};
    use crate::config::StarkConfig;
    use crate::cpu::cpu_stark::CpuStark;
    use crate::cross_table_lookup::CrossTableLookup;
    use crate::keccak::keccak_stark::{KeccakStark, INPUT_LIMBS, NUM_ROUNDS};
    use crate::prover::prove;
    use crate::verifier::verify_proof;

    #[test]
    fn test_all_stark() -> Result<()> {
        const D: usize = 2;
        type C = PoseidonGoldilocksConfig;
        type F = <C as GenericConfig<D>>::F;

        let config = StarkConfig::standard_fast_config();

        let cpu_stark = CpuStark::<F, D> {
            f: Default::default(),
        };
        let cpu_rows = 1 << 6;

        let keccak_stark = KeccakStark::<F, D> {
            f: Default::default(),
        };
        let keccak_rows = (NUM_ROUNDS + 1).next_power_of_two();

        let mut cpu_trace = vec![PolynomialValues::<F>::zero(cpu_rows); 10];

        let mut rng = ChaCha8Rng::seed_from_u64(0x6feb51b7ec230f25);

        let num_inpts = 1;
        let keccak_inputs = (0..num_inpts)
            .map(|_| [0u64; INPUT_LIMBS].map(|_| rng.gen()))
            .collect_vec();
        let keccak_trace = keccak_stark.generate_trace(keccak_inputs);

        let vs0: Vec<_> = keccak_trace[3].values[..].into();
        let vs1: Vec<_> = keccak_trace[5].values[..].into();

        let start = thread_rng().gen_range(0..cpu_rows - keccak_rows);

        let default = vec![F::ONE; 2];

        cpu_trace[2].values = vec![default[0]; cpu_rows];
        cpu_trace[2].values[start..start + keccak_rows].copy_from_slice(&vs0);
        cpu_trace[4].values = vec![default[1]; cpu_rows];
        cpu_trace[4].values[start..start + keccak_rows].copy_from_slice(&vs1);

        let cross_table_lookups = vec![CrossTableLookup {
            looking_table: Table::Cpu,
            looking_columns: vec![2, 4],
            looked_table: Table::Keccak,
            looked_columns: vec![3, 5],
            default: vec![F::ONE; 2],
        }];

        let all_stark = AllStark {
            cpu_stark,
            keccak_stark,
            cross_table_lookups,
        };

        let proof = prove::<F, C, D>(
            &all_stark,
            &config,
            vec![cpu_trace, keccak_trace],
            vec![vec![]; 2],
            &mut TimingTree::default(),
        )?;

        verify_proof(all_stark, proof, &config)
    }
}
