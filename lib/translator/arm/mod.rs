//! Capstone-based translator for 32-bit x86.

use falcon_capstone::{capstone, capstone_sys};
use falcon_capstone::capstone_sys::arm_op_type;
use error::*;
use il::*;
use translator::{Translator, BlockTranslationResult};

#[cfg(test)] mod test;
mod semantics;

/// The X86 translator.
#[derive(Clone, Debug)]
pub struct Arm;


impl Arm {
    pub fn new() -> Arm {
        Arm
    }
}


impl Translator for Arm {
    fn translate_block(&self, bytes: &[u8], address: u64) -> Result<BlockTranslationResult> {
        let cs = match capstone::Capstone::new(capstone::cs_arch::CS_ARCH_ARM,
                                               capstone::CS_MODE_ARM) {
            Ok(cs) => cs,
            Err(_) => return Err("Capstone Error".into())
        };

        cs.option(capstone::cs_opt_type::CS_OPT_DETAIL, capstone::cs_opt_value::CS_OPT_ON).unwrap();

        // A vec which holds each lifted instruction in this block.
        let mut block_graphs: Vec<(u64, ControlFlowGraph)> = Vec::new();

        // the length of this block in bytes
        let mut length: usize = 0;

        let mut successors = Vec::new();

        let mut offset: usize = 0;

        loop {
            /* We must have at least 16 bytes left in the buffer. */
            // if bytes.len() - offset < 16 {
            //     successors.push((address + offset as u64, None));
            //     break;
            // }
            let disassembly_range = (offset)..bytes.len();
            let disassembly_bytes = bytes.get(disassembly_range).unwrap();
            let instructions = match cs.disasm(disassembly_bytes, address + offset as u64, 1) {
                Ok(instructions) => instructions,
                Err(e) => match e.code() {
                    capstone_sys::cs_err::CS_ERR_OK => {
                        successors.push((address + offset as u64, None));
                        break;
                    }
                    _ => bail!("Capstone Error: {}", e.code() as u32)
                }
            };

            if instructions.count() == 0 {
                return Err("Capstone failed to disassemble any instruction".into());
            }

            let instruction = instructions.get(0).unwrap();

            if let capstone::InstrIdArch::ARM(instruction_id) = instruction.id {
                
                let mut instruction_graph = ControlFlowGraph::new();

                try!(match instruction_id {
                    capstone::arm_insn::ARM_INS_ADC  => semantics::adc(&mut instruction_graph, &instruction),
                    capstone::arm_insn::ARM_INS_ADD  => semantics::add(&mut instruction_graph, &instruction),
                    capstone::arm_insn::ARM_INS_ADR  => semantics::adr(&mut instruction_graph, &instruction),
                    capstone::arm_insn::ARM_INS_AND  => semantics::and(&mut instruction_graph, &instruction),
                    capstone::arm_insn::ARM_INS_ASR  => semantics::asr(&mut instruction_graph, &instruction),
                    capstone::arm_insn::ARM_INS_BFC  => semantics::bfc(&mut instruction_graph, &instruction),
                    capstone::arm_insn::ARM_INS_BFI  => semantics::bfi(&mut instruction_graph, &instruction),
                    capstone::arm_insn::ARM_INS_BIC  => semantics::bic(&mut instruction_graph, &instruction),
                    capstone::arm_insn::ARM_INS_BKPT  => semantics::bkpt(&mut instruction_graph, &instruction),
                    capstone::arm_insn::ARM_INS_BL   => semantics::bl(&mut instruction_graph, &instruction),
                    capstone::arm_insn::ARM_INS_BLX  => semantics::blx(&mut instruction_graph, &instruction),
                    capstone::arm_insn::ARM_INS_CLZ  => semantics::clz(&mut instruction_graph, &instruction),
                    capstone::arm_insn::ARM_INS_SUB  => semantics::sub(&mut instruction_graph, &instruction),
                    _ => return Err(format!("Unhandled instruction {} at 0x{:x}",
                        instruction.mnemonic,
                        instruction.address
                    ).into())
                });

                instruction_graph.set_address(Some(instruction.address));

                block_graphs.push((instruction.address, instruction_graph));

                length += instruction.size as usize;

                match instruction_id {
                    capstone::arm_insn::ARM_INS_B => {
                        let detail = semantics::details(&instruction)?;
                        assert!(detail.operands[0].type_ == arm_op_type::ARM_OP_IMM);
                        successors.push((detail.operands[0].imm() as u64, None));
                        break;
                    },
                    capstone::arm_insn::ARM_INS_BX |
                    capstone::arm_insn::ARM_INS_BXJ => {
                        let detail = semantics::details(&instruction)?;
                        assert!(detail.operands[0].type_ == arm_op_type::ARM_OP_IMM);
                        successors.push((detail.operands[0].imm() as u64 + 1, None));
                        break;
                    },
                    capstone::arm_insn::ARM_INS_CBNZ => {
                        let detail = semantics::details(&instruction)?;
                        assert!(detail.operands[1].type_ == arm_op_type::ARM_OP_IMM);
                        let register = semantics::get_register_expression(&instruction, 0)?;
                        let condition = Expression::cmpneq(register.clone(),
                                                           expr_const(0, register.bits()))?;
                        successors.push((
                            detail.operands[1].imm() as u64,
                            Some(condition.clone())
                        ));
                        successors.push((
                            instruction.address as u64 + 4,
                            Some(Expression::cmpeq(condition, expr_const(0, 1))?)
                        ));
                        break; 
                    },
                    capstone::arm_insn::ARM_INS_CBZ => {
                        let detail = semantics::details(&instruction)?;
                        assert!(detail.operands[1].type_ == arm_op_type::ARM_OP_IMM);
                        let register = semantics::get_register_expression(&instruction, 0)?;
                        let condition = Expression::cmpeq(register.clone(),
                                                          expr_const(0, register.bits()))?;
                        successors.push((
                            detail.operands[1].imm() as u64,
                            Some(condition.clone())
                        ));
                        successors.push((
                            instruction.address as u64 + 4,
                            Some(Expression::cmpeq(condition, expr_const(0, 1))?)
                        ));
                        break; 
                    },
                    _ => {}
                }
            }
            else {
                bail!("not an x86 instruction")
            }

            offset += instruction.size as usize;
        }

        Ok(BlockTranslationResult::new(block_graphs, address, length, successors))
    }
}
