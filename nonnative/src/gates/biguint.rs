use core::marker::PhantomData;
use core::ops::Range;
use num::bigint::BigUint;
// use plonky2::plonk::circuit_data::CircuitConfig;

use plonky2::field::extension::Extendable;
// use plonky2::field::types::{Field, PrimeField};
// use plonky2::field::secp256k1_scalar::Secp256K1Scalar;
use plonky2::gates::gate::Gate;
use plonky2::gates::util::StridedConstraintConsumer;
use plonky2::hash::hash_types::RichField;

use plonky2::iop::ext_target::ExtensionTarget;
// use plonky2::iop::generator::WitnessGenerator;
use plonky2::iop::generator::{GeneratedValues, SimpleGenerator, WitnessGenerator};
use plonky2::iop::witness::{PartitionWitness, Witness, WitnessWrite};

use plonky2::iop::target::Target;

use plonky2::plonk::circuit_builder::CircuitBuilder;
use plonky2::plonk::vars::{EvaluationVars, EvaluationVarsBase, EvaluationTargets};

// use plonky2_ecdsa::gadgets::biguint::BigUintTarget;
// use plonky2_u32::gadgets::arithmetic_u32::U32Target;

// use plonky2_ecdsa::gadgets::nonnative::{CircuitBuilderBigUint, BigUintTarget};
use plonky2_ecdsa::gadgets::biguint::CircuitBuilderBiguint;

use crate::gates::vars::FieldExtToBigUint;
use crate::gates::vars::BigUintToVecFieldExt;

use crate::gates::vars::FieldToBigUint;
use crate::gates::vars::BigUintToVecField;

use crate::gates::vars::FieldExtTargetsToBigUintTarget;
use crate::gates::vars::BigUintTargetToVecFieldExtTargets;

/// A gate which can perform multiplication of two BigUint values, i.e. `result = x y`.
#[derive(Copy, Clone, Debug)]
pub struct MulBigUintGate<F: RichField + Extendable<D>, const D: usize> { 
    pub multiplicand0_num_limbs: usize,
    pub multiplicand1_num_limbs: usize,
    pub total_input_limbs: usize,
    _phantom: PhantomData<F>,
}

impl<F: RichField + Extendable<D>, const D: usize> MulBigUintGate<F, D> {
    pub fn new(multiplicand0_num_limbs: usize, multiplicand1_num_limbs: usize) -> Self {
        let total_input_limbs = multiplicand0_num_limbs + multiplicand1_num_limbs;
println!("total_input_limbs  = {}", total_input_limbs);
        Self {
            multiplicand0_num_limbs,
            multiplicand1_num_limbs,
            total_input_limbs,
            _phantom: PhantomData,
        }
    }

    pub fn wire_ith_limb_of_multiplicand_0(&self, i: usize) -> usize {
        debug_assert!(i < self.multiplicand0_num_limbs);
        i
    }

    pub fn wire_ith_limb_of_multiplicand_1(&self, i: usize) -> usize {
        debug_assert!(i < self.multiplicand1_num_limbs);
        self.multiplicand0_num_limbs + i
    }

    pub fn wire_ith_limb_of_output(&self, i: usize) -> usize {
        debug_assert!(i < self.total_input_limbs);
        self.total_input_limbs + i
    }

    pub fn wires_multiplicand_0(&self) -> Range<usize> {
        0..self.multiplicand0_num_limbs
    }

    pub fn wires_multiplicand_1(&self) -> Range<usize> {
        self.multiplicand0_num_limbs..self.total_input_limbs
    }

    pub fn wires_output(&self) -> Range<usize> {
        self.total_input_limbs..self.total_input_limbs * 2
    }
}

impl<F: RichField + Extendable<D>, const D: usize> Gate<F, D> for MulBigUintGate<F, D> {
    fn id(&self) -> String {
        format!("{self:?}")
    }

    fn export_circom_verification_code(&self) -> String {
        todo!()
    }

    fn export_solidity_verification_code(&self) -> String {
        todo!()
    }

    fn eval_unfiltered(&self, vars: EvaluationVars<F, D>) -> Vec<F::Extension> {
        let mut constraints = Vec::new();

        let multiplicand_0 = vars.get_local_biguint_algebra(self.wires_multiplicand_0());
        let multiplicand_1 = vars.get_local_biguint_algebra(self.wires_multiplicand_1());
        let output = vars.get_local_biguint_algebra(self.wires_output());
        let computed_output = multiplicand_0 * multiplicand_1;

        constraints.extend(<BigUint as BigUintToVecFieldExt<F, D>>::to_basefield_array(&(output - computed_output)));

        constraints
    }

    fn eval_unfiltered_base_one(
        &self,
        vars: EvaluationVarsBase<F>,
        mut yield_constr: StridedConstraintConsumer<F>,
    ) {
        let multiplicand_0 = vars.get_local_biguint(self.wires_multiplicand_0());
        let multiplicand_1 = vars.get_local_biguint(self.wires_multiplicand_1());
        let output = vars.get_local_biguint(self.wires_output());
println!("m0 = {:#?}", multiplicand_0);
println!("m1 = {:#?}", multiplicand_1);
        let computed_output = multiplicand_0 * multiplicand_1;

println!("ou = {:#?}", output);
println!("co = {:#?}", computed_output);

        yield_constr.many(<BigUint as BigUintToVecField<F>>::to_basefield_array(&(output - computed_output)));
    }

    fn eval_unfiltered_circuit(
        &self,
        builder: &mut CircuitBuilder<F, D>,
        vars: EvaluationTargets<D>,
    ) -> Vec<ExtensionTarget<D>> {
        let mut constraints = Vec::new();

        let multiplicand_0 = vars.get_local_biguint_algebra(self.wires_multiplicand_0());
        let multiplicand_1 = vars.get_local_biguint_algebra(self.wires_multiplicand_1());
        let output = vars.get_local_biguint_algebra(self.wires_output());
        let computed_output = builder.mul_biguint(&multiplicand_0, &multiplicand_1);

        let diff = builder.sub_biguint(&output, &computed_output);
        constraints.extend(diff.to_ext_target_array());

        constraints
    }

    fn generators(&self, row: usize, _local_constants: &[F]) -> Vec<Box<dyn WitnessGenerator<F>>> {
        let gen = MulBigUintGenerator { gate: *self, row };
        vec![Box::new(gen.adapter())]
    }

    fn num_wires(&self) -> usize {
        self.total_input_limbs * 2
    }

    fn num_constants(&self) -> usize {
        0
    }

    fn degree(&self) -> usize {
        1 /* ? */
    }

    fn num_constraints(&self) -> usize {
        self.total_input_limbs * 2
    }
}

#[derive(Debug)]
pub struct MulBigUintGenerator<F: RichField + Extendable<D>, const D: usize> {
    gate: MulBigUintGate<F, D>,
    row: usize,
}

impl<F: RichField + Extendable<D>, const D: usize> SimpleGenerator<F>
    for MulBigUintGenerator<F, D>
{
    fn dependencies(&self) -> Vec<Target> {
        let mut m0: Vec<Target> = self.gate
                                        .wires_multiplicand_0()
                                        .map(|i| Target::wire(self.row, i))
                                        .collect();

        let m1: Vec<Target> = self.gate
                                    .wires_multiplicand_1()
                                    .map(|i| Target::wire(self.row, i))
                                    .collect();

        m0.extend(m1);

        m0
    }

    fn run_once(&self, witness: &PartitionWitness<F>, out_buffer: &mut GeneratedValues<F>) {
        fn mul_u32(a: u32, b: u32) -> (u32, u32) {
            let a = a as u64;
            let b = b as u64;
            let product = a * b;
            let carry = u32::try_from(product >> 32).unwrap();
            let product = u32::try_from(product & 0xffffffffu64).unwrap();

            (product, carry)
        }

        fn add_u32(a: u32, b: u32) -> (u32, u32) {
            let a = a as u64;
            let b = b as u64;
            let sum = a + b;
            let carry = u32::try_from(sum >> 32).unwrap();
            let sum = u32::try_from(sum & 0xffffffffu64).unwrap();

            (sum, carry)
        }

        fn add_u32s_with_carry(to_add: &[u32], carry: u32) -> (u32, u32) {
            if to_add.len() == 1 {
                return add_u32(to_add[0], carry);
            }

            let to_add: Vec<u64> = (*to_add).iter().map(|v| *v as u64).collect();
            let sum: u64 = to_add.iter().sum();
            let carry = u32::try_from(sum >> 32).unwrap();
            let sum = u32::try_from(sum & 0xffffffffu64).unwrap();

            (sum, carry)
        }

        let m0: Vec<u32> = self.gate
                                .wires_multiplicand_0()
                                .map(|i| {
                                    witness
                                        .get_target(Target::wire(self.row, i))
                                        .to_canonical_u64() as u32
                                })
                                .collect();
                                        
println!("generator m0 = {:?}", m0);

        let m1: Vec<u32> = self.gate
                                .wires_multiplicand_1()
                                .map(|i| {
                                    witness
                                        .get_target(Target::wire(self.row, i))
                                        .to_canonical_u64() as u32
                                })
                                .collect();

println!("generator m1 = {:?}", m1);

        let mut to_add = vec![vec![]; self.gate.total_input_limbs];

        let m0_num_limbs = self.gate.multiplicand0_num_limbs;
        let m1_num_limbs = self.gate.multiplicand1_num_limbs;

        for i in 0..m0_num_limbs {
            for j in 0..m1_num_limbs {
                let (product, carry) = mul_u32(m0[i], m1[j]);

                to_add[i + j].push(product);
                to_add[i + j + 1].push(carry);
            }
        }

        let mut limb_values = vec![];
        let mut carry = 0_u32;

        for summands in &mut to_add {
            let (new_product, new_carry) = add_u32s_with_carry(summands, carry);
            limb_values.push(new_product);
            carry = new_carry;
        }

        assert_eq!(carry, 0);

        let output_limbs: Vec<Target> = self.gate
                                            .wires_output()
                                            .map(|i| Target::wire(self.row, i))
                                            .collect();

        let output_limb_values: Vec<F> = limb_values.iter().map(|v| F::from_canonical_u32(*v)).collect();

        for (l, v) in output_limbs.iter().zip(output_limb_values) {
println!("generator output = {}", v);
            out_buffer.set_target(*l, v);
        }
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    // use num::bigint::BigUint;
    // use plonky2::field::types::Sample;
    // use plonky2::iop::witness::PartialWitness;
    // use plonky2::plonk::circuit_data::CircuitConfig;
    // use plonky2::plonk::config::{GenericConfig, PoseidonGoldilocksConfig}; 

    // use plonky2::field::secp256k1_base::Secp256K1Base;

    // use super::*;

    #[test]
    fn test_biguint_gate() -> Result<()> {
        todo!()
    }
}