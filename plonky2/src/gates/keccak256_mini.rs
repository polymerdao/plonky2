use alloc::string::String;
use alloc::vec::Vec;
use alloc::{format, vec};
use core::marker::PhantomData;
use core::ops::Range;

use plonky2_field::extension::algebra::ExtensionAlgebra;
use plonky2_field::extension::{FieldExtension, OEF};

use crate::field::extension::Extendable;
use crate::field::types::Field;
use crate::gates::gate::Gate;
use crate::gates::util::StridedConstraintConsumer;
use crate::hash::hash_types::RichField;
use crate::iop::ext_target::ExtensionTarget;
use crate::iop::generator::{GeneratedValues, SimpleGenerator, WitnessGeneratorRef};
use crate::iop::target::{BoolTarget, Target};
use crate::iop::wire::Wire;
use crate::iop::witness::{PartitionWitness, Witness, WitnessWrite};
use crate::plonk::circuit_builder::CircuitBuilder;
use crate::plonk::vars::{EvaluationTargets, EvaluationVars, EvaluationVarsBase};
use crate::util::serialization::{Buffer, IoResult, Read, Write};

pub const WIDTH: usize = 5;
pub const STATE_SIZE: usize = WIDTH * WIDTH;

fn xor<F: Field>(a: F, b: F) -> F {
    // a + b - 2ab
    let ab = a * b;

    a + b - ab.double()
}

fn xor_ext_algebra<F: OEF<D>, const D: usize>(
    a: ExtensionAlgebra<F, D>,
    b: ExtensionAlgebra<F, D>,
) -> ExtensionAlgebra<F, D> {
    // a + b - 2ab
    let ab = a * b;

    a + b - ab - ab
}

/// Same as `mds_row_shf` for an extension algebra of `F`.
pub fn calc_xor5<F: Field>(input: [F; WIDTH]) -> F {
    let xor01 = xor(input[0], input[1]);
    let xor012 = xor(xor01, input[2]);
    let xor34 = xor(input[3], input[4]);

    xor(xor012, xor34)
}

/// https://github.com/polymerdao/plonky2-sha256/blob/main/src/circuit.rs#L105
/// a ^ b ^ c = a+b+c - 2*a*b - 2*a*c - 2*b*c + 4*a*b*c
///           = a*( 1 - 2*b - 2*c + 4*b*c ) + b + c - 2*b*c
///           = a*( 1 - 2*b -2*c + 4*m ) + b + c - 2*m
/// where m = b*c
fn xor3<F: RichField + Extendable<D>, const D: usize>(
    builder: &mut CircuitBuilder<F, D>,
    a: BoolTarget,
    b: BoolTarget,
    c: BoolTarget,
) -> BoolTarget {
    let m = builder.mul(b.target, c.target);
    let two_b = builder.add(b.target, b.target);
    let two_c = builder.add(c.target, c.target);
    let two_m = builder.add(m, m);
    let four_m = builder.add(two_m, two_m);
    let one = builder.one();
    let one_sub_two_b = builder.sub(one, two_b);
    let one_sub_two_b_sub_two_c = builder.sub(one_sub_two_b, two_c);
    let one_sub_two_b_sub_two_c_add_four_m = builder.add(one_sub_two_b_sub_two_c, four_m);
    let mut res = builder.mul(a.target, one_sub_two_b_sub_two_c_add_four_m);
    res = builder.add(res, b.target);
    res = builder.add(res, c.target);

    BoolTarget::new_unsafe(builder.sub(res, two_m))
}

/// Keccak256 rho and pi Gate
/// 5 times xor
#[derive(Debug, Default)]
pub struct Xor5Gate<F: RichField + Extendable<D>, const D: usize>(PhantomData<F>);

impl<F: RichField + Extendable<D>, const D: usize> Xor5Gate<F, D> {
    pub fn new() -> Self {
        Self(PhantomData)
    }

    pub fn wires_input(i: usize) -> Range<usize> {
        assert!(i < WIDTH);

        i * D..(i + 1) * D
    }

    pub fn wires_output() -> Range<usize> {
        WIDTH * D..(WIDTH + 1) * D
    }

    pub fn end() -> usize {
        (WIDTH + 1) * D
    }

    // /// Same as `mds_row_shf_recursive` for an extension algebra of `F`.
    // fn calc_theta_circuit(
    //     builder: &mut CircuitBuilder<F, D>,
    //     input: &[U64ExtensionTarget<D>; WIDTH],
    // ) -> [U64ExtensionTarget<D>; WIDTH] {
    //     let mut output = vec![];
    //     for x in 0..5 {
    //         let xor01 = input[x].xor(&input[x + 5], builder);
    //         let xor012 = xor01.xor(&input[x + 2 * 5], builder);
    //         let xor0123 = xor012.xor(&input[x + 3 * 5], builder);
    //         let xor01234 = xor0123.xor(&input[x + 4 * 5], builder);
    //         output.push(xor01234); // bind, deg-3
    //     }

    //     output
    // }
}

impl<F: RichField + Extendable<D>, const D: usize> Gate<F, D> for Xor5Gate<F, D> {
    fn id(&self) -> String {
        format!("{self:?}")
    }

    fn serialize(&self, _dst: &mut Vec<u8>) -> IoResult<()> {
        Ok(())
    }

    fn deserialize(_src: &mut Buffer) -> IoResult<Self> {
        Ok(Xor5Gate::new())
    }

    fn eval_unfiltered(&self, vars: EvaluationVars<F, D>) -> Vec<F::Extension> {
        let inputs: [_; WIDTH] = (0..WIDTH)
            .map(|i| vars.get_local_ext_algebra(Self::wires_input(i)))
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        let output = vars.get_local_ext_algebra(Self::wires_output());

        let xor01 = xor_ext_algebra(inputs[0], inputs[1]);
        let xor012 = xor_ext_algebra(xor01, inputs[2]);
        let xor0123 = xor_ext_algebra(xor012, inputs[3]);
        let xor01234 = xor_ext_algebra(xor0123, inputs[4]);

        (xor01234 - output).to_basefield_array().to_vec()
    }

    fn eval_unfiltered_base_one(
        &self,
        vars: EvaluationVarsBase<F>,
        mut yield_constr: StridedConstraintConsumer<F>,
    ) {
        let inputs: [_; WIDTH] = (0..WIDTH)
            .map(|i| vars.get_local_ext(Self::wires_input(i)))
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();

        let output = vars.get_local_ext(Self::wires_output());

        let xor01 = xor(inputs[0], inputs[1]);
        let xor012 = xor(xor01, inputs[2]);
        let xor0123 = xor(xor012, inputs[3]);
        let xor01234 = xor(xor0123, inputs[4]);

        yield_constr.many((xor01234 - output).to_basefield_array());
    }

    fn eval_unfiltered_circuit(
        &self,
        builder: &mut CircuitBuilder<F, D>,
        vars: EvaluationTargets<D>,
    ) -> Vec<ExtensionTarget<D>> {
        let inputs: [_; WIDTH] = (0..WIDTH)
            .map(|i| vars.get_local_ext_algebra(Self::wires_input(i)))
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        let output = vars.get_local_ext_algebra(Self::wires_output());

        let xor01 = builder.xor_ext_algebra(inputs[0], inputs[1]);
        let xor012 = builder.xor_ext_algebra(xor01, inputs[2]);
        let xor0123 = builder.xor_ext_algebra(xor012, inputs[3]);
        let xor01234 = builder.xor_ext_algebra(xor0123, inputs[4]);

        builder
            .sub_ext_algebra(xor01234, output)
            .to_ext_target_array()
            .to_vec()
    }

    fn generators(&self, row: usize, _local_constants: &[F]) -> Vec<WitnessGeneratorRef<F>> {
        let gen = Xor5Generator {
            row,
            _phantom: PhantomData,
        };
        vec![WitnessGeneratorRef::new(gen.adapter())]
    }

    fn num_wires(&self) -> usize {
        Self::end()
    }

    fn num_constants(&self) -> usize {
        0
    }

    fn degree(&self) -> usize {
        5
    }

    fn num_constraints(&self) -> usize {
        2
    }
}

#[derive(Clone, Debug, Default)]
pub struct Xor5Generator<F: RichField + Extendable<D>, const D: usize> {
    row: usize,
    _phantom: PhantomData<F>,
}

impl<F: RichField + Extendable<D>, const D: usize> SimpleGenerator<F> for Xor5Generator<F, D> {
    fn id(&self) -> String {
        "Xor5Generator".to_string()
    }

    fn dependencies(&self) -> Vec<Target> {
        (0..WIDTH)
            .flat_map(|i| {
                Target::wires_from_range(self.row, Xor5Gate::<F, D>::wires_input(i))
            })
            .collect()
    }

    fn run_once(&self, witness: &PartitionWitness<F>, out_buffer: &mut GeneratedValues<F>) {
        let get_local_get_target = |wire_range| ExtensionTarget::from_range(self.row, wire_range);
        let get_local_ext =
            |wire_range| witness.get_extension_target(get_local_get_target(wire_range));

        let inputs: [_; WIDTH] = (0..WIDTH)
            .map(|i| get_local_ext(Xor5Gate::<F, D>::wires_input(i)))
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();

        let computed_output = calc_xor5(inputs);

        out_buffer.set_extension_target(
            get_local_get_target(Xor5Gate::<F, D>::wires_output()),
            computed_output,
        );
    }

    fn serialize(&self, dst: &mut Vec<u8>) -> IoResult<()> {
        dst.write_usize(self.row)
    }

    fn deserialize(src: &mut Buffer) -> IoResult<Self> {
        let row = src.read_usize()?;
        Ok(Self {
            row,
            _phantom: PhantomData,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Xor5Gate;
    use crate::gates::gate_testing::{test_eval_fns, test_low_degree};
    use crate::plonk::config::{GenericConfig, PoseidonGoldilocksConfig};

    #[test]
    fn low_degree() {
        const D: usize = 2;
        type C = PoseidonGoldilocksConfig;
        type F = <C as GenericConfig<D>>::F;
        let gate = Xor5Gate::<F, D>::new();
        test_low_degree(gate)
    }

    #[test]
    fn eval_fns() -> anyhow::Result<()> {
        const D: usize = 2;
        type C = PoseidonGoldilocksConfig;
        type F = <C as GenericConfig<D>>::F;
        let gate = Xor5Gate::<F, D>::new();
        test_eval_fns::<F, C, _, D>(gate)
    }
}
