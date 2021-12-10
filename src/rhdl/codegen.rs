// Copyright (c) 2016-2021 Fabian Schuiki

//! LLHD code generation for VHDL.

use crate::hir;
use crate::konst::*;
use crate::score::*;
use crate::ty::*;
use llhd;
use moore_common::errors::*;
use moore_common::score::Result;
use num::{Signed, ToPrimitive, Zero};

/// Generates LLHD code.
pub trait Codegen<I, C> {
    fn codegen(&self, id: I, ctx: &mut C) -> Result<()>;
}

/// This macro implements the `Codegen` trait for a specific combination of
/// identifier and context types.
macro_rules! impl_codegen {
    ($slf:tt, $id:ident: $id_ty:ty, $ctx:ident: &mut $ctx_ty:ty => $blk:block) => {
        impl<'lazy, 'sb, 'ast, 'ctx> Codegen<$id_ty, $ctx_ty> for ScoreContext<'lazy, 'sb, 'ast, 'ctx> {
            fn codegen(&$slf, $id: $id_ty, $ctx: &mut $ctx_ty) -> Result<()> $blk
        }
    };

    ($slf:tt, $id:ident: $id_ty:ty, $ctx:ident: &$ctx_lt:tt mut $ctx_ty:ty => $blk:block) => {
        impl<'lazy, 'sb, 'ast, 'ctx, $ctx_lt> Codegen<$id_ty, $ctx_ty> for ScoreContext<'lazy, 'sb, 'ast, 'ctx> {
            fn codegen(&$slf, $id: $id_ty, $ctx: &mut $ctx_ty) -> Result<()> $blk
        }
    }
}

macro_rules! unimp {
    ($slf:tt, $id:expr) => {{
        $slf.sess.emit(DiagBuilder2::bug(format!(
            "code generation for {:?} not implemented",
            $id
        )));
        return Err(());
    }};
}

impl<'lazy, 'sb, 'ast, 'ctx> ScoreContext<'lazy, 'sb, 'ast, 'ctx> {
    /// Map a VHDL type to the corresponding LLHD type.
    pub fn map_type(&self, ty: &Ty) -> Result<llhd::Type> {
        let ty = self.deref_named_type(ty)?;
        Ok(match *ty {
            Ty::Named(..) => unreachable!(),
            Ty::Null => llhd::void_ty(),
            Ty::Int(ref ty) => {
                let diff = match ty.dir {
                    hir::Dir::To => &ty.right_bound - &ty.left_bound,
                    hir::Dir::Downto => &ty.left_bound - &ty.right_bound,
                };
                if diff.is_negative() {
                    llhd::void_ty()
                } else {
                    llhd::int_ty(diff.bits() as usize)
                }
            }
            Ty::Enum(ref ty) => {
                let hir = self.lazy_hir(ty.decl)?;
                match hir.data.as_ref().unwrap().value {
                    hir::TypeData::Enum(ref lits) => llhd::enum_ty(lits.len()),
                    _ => unreachable!(),
                }
            }
            Ty::Physical(ref ty) => {
                self.emit(DiagBuilder2::error(format!(
                    "cannot generate code for physical type `{}`",
                    ty
                )));
                return Err(());
            }
            Ty::Access(ref ty) => llhd::pointer_ty(self.map_type(ty)?),
            Ty::Array(ref ty) => {
                let mut llty = self.map_type(&ty.element)?;
                for index in ty.indices.iter().rev() {
                    match *index {
                        ArrayIndex::Unbounded(_) => {
                            self.emit(
                                DiagBuilder2::error(format!("type `{}` is unbounded", ty)), // TODO: What span should we use here?
                            );
                            return Err(());
                        }
                        ArrayIndex::Constrained(ref ty) => {
                            let num = match **ty {
                                Ty::Int(ref ty) => {
                                    let l = ty.len();
                                    if l.is_negative() || l.is_zero() {
                                        return Ok(llhd::void_ty());
                                    }
                                    match l.to_usize() {
                                        Some(l) => l,
                                        None => {
                                            self.emit(
                                                DiagBuilder2::error(format!(
                                                    "array index `{}` is too large; {} elements",
                                                    ty, l
                                                )), // TODO: What span should we use here?
                                            );
                                            return Err(());
                                        }
                                    }
                                }
                                Ty::Enum(ref ty) => {
                                    match self.lazy_hir(ty.decl)?.data.as_ref().unwrap().value {
                                        hir::TypeData::Enum(ref lits) => lits.len(),
                                        _ => unreachable!(),
                                    }
                                }
                                _ => {
                                    self.emit(
                                        DiagBuilder2::error(format!(
                                            "`{}` is an invalid array index type",
                                            ty
                                        )), // TODO: What span should we use here?
                                    );
                                    return Err(());
                                }
                            };
                            llty = llhd::array_ty(num, llty);
                        }
                    }
                }
                llty
            }
            Ty::File(ref _ty) => llhd::int_ty(32),
            Ty::Record(ref ty) => {
                let fields = ty
                    .fields
                    .iter()
                    .map(|&(_, ref ty)| self.map_type(ty))
                    .collect::<Result<_>>()?;
                llhd::struct_ty(fields)
            }
            Ty::Subprog(..) => unimplemented!(),
            // Unbounded integers cannot be mapped to LLHD. All cases where
            // such an int can leak through to codegen should actually be caught
            // beforehand in the type check.
            Ty::UnboundedInt | Ty::UniversalInt => unreachable!(),
        })
    }

    /// Map a constant value to the LLHD counterpart.
    pub fn map_const(
        &self,
        builder: &mut llhd::ir::UnitBuilder,
        konst: &Const,
    ) -> Result<llhd::ir::Value> {
        Ok(match *konst {
            // TODO: Map this to llhd::const_void once available.
            Const::Null => builder.ins().const_int((0, 0)),
            Const::Int(ref k) => builder.ins().const_int((999, k.value.clone())),
            Const::Enum(ref k) => {
                let size = match self.lazy_hir(k.decl)?.data.as_ref().unwrap().value {
                    hir::TypeData::Enum(ref lits) => lits.len(),
                    _ => unreachable!(),
                };
                builder.ins().const_int((size, k.index))
            }
            Const::Float(ref _k) => panic!("cannot map float constant"),
            Const::IntRange(_) | Const::FloatRange(_) => panic!("cannot map range constant"),
        }
        .into())
    }
}

impl_codegen!(self, id: DeclInBlockRef, ctx: &mut llhd::ir::UnitBuilder<'_> => {
    match id {
        DeclInBlockRef::Subprog(id)     => self.codegen(id, &mut ()),
        DeclInBlockRef::SubprogBody(id) => self.codegen(id, &mut ()),
        DeclInBlockRef::SubprogInst(id) => self.codegen(id, &mut ()),
        DeclInBlockRef::Pkg(id)         => self.codegen(id, &mut ()),
        DeclInBlockRef::PkgBody(id)     => self.codegen(id, &mut ()),
        DeclInBlockRef::PkgInst(id)     => self.codegen(id, &mut ()),
        DeclInBlockRef::Type(_id)       => Ok(()),
        DeclInBlockRef::Subtype(_id)    => Ok(()),
        DeclInBlockRef::Const(id)       => self.codegen(id, ctx),
        DeclInBlockRef::Signal(id)      => self.codegen(id, ctx),
        DeclInBlockRef::Var(id)         => self.codegen(id, ctx),
        DeclInBlockRef::File(id)        => self.codegen(id, ctx),
        DeclInBlockRef::Alias(_id)      => Ok(()),
        DeclInBlockRef::Comp(id)        => self.codegen(id, &mut ()),
        DeclInBlockRef::Attr(_id)       => Ok(()),
        DeclInBlockRef::AttrSpec(_id)   => Ok(()),
        DeclInBlockRef::CfgSpec(_id)    => Ok(()),
        DeclInBlockRef::Discon(_id)     => Ok(()),
        DeclInBlockRef::GroupTemp(_id)  => Ok(()),
        DeclInBlockRef::Group(_id)      => Ok(()),
    }
});

impl_codegen!(self, id: ConstDeclRef, _ctx: &mut llhd::ir::UnitBuilder<'_> => {
    unimp!(self, id);
});

impl_codegen!(self, id: VarDeclRef, _ctx: &mut llhd::ir::UnitBuilder<'_> => {
    unimp!(self, id);
});

impl_codegen!(self, id: SignalDeclRef, ctx: &mut llhd::ir::UnitBuilder<'_> => {
    // Determine the type of the signal.
    let hir = self.lazy_hir(id)?;
    let ty = self.lazy_typeval(id)?;

    // Calculate the initial value for the signal, either from the provided
    // expression or implicitly.
    let init = if let Some(init_id) = hir.decl.init {
        self.const_value(init_id)?
    } else {
        self.default_value_for_type(&ty)?
    };

    debugln!("signal {:?}, type {:?}, init {:?}", id, ty, init);
    // Create the signal instance.
    // let inst = llhd::Inst::new(
    //     Some(hir.name.value.into()),
    //     llhd::SignalInst(self.map_type(ty)?, Some(self.map_const(init)?))
    // );
    // ctx.add_inst(inst, llhd::InstPosition::End);
    let k = self.map_const(ctx, init)?;
    ctx.ins().sig(k);
    Ok(())
});

impl_codegen!(self, id: FileDeclRef, _ctx: &mut llhd::ir::UnitBuilder<'_> => {
    unimp!(self, id);
});

impl_codegen!(self, id: ConcStmtRef, ctx: &mut llhd::ir::UnitBuilder<'_> => {
    match id {
        ConcStmtRef::Block(id)         => self.codegen(id, ctx),
        ConcStmtRef::Process(id)       => self.codegen(id, ctx),
        ConcStmtRef::ConcProcCall(id)  => self.codegen(id, ctx),
        ConcStmtRef::ConcAssert(id)    => self.codegen(id, ctx),
        ConcStmtRef::ConcSigAssign(id) => self.codegen(id, ctx),
        ConcStmtRef::CompInst(id)      => self.codegen(id, ctx),
        ConcStmtRef::ForGen(id)        => self.codegen(id, ctx),
        ConcStmtRef::IfGen(id)         => self.codegen(id, ctx),
        ConcStmtRef::CaseGen(id)       => self.codegen(id, ctx),
    }
});

impl_codegen!(self, id: BlockStmtRef, _ctx: &mut llhd::ir::UnitBuilder<'_> => {
    unimp!(self, id);
});

impl_codegen!(self, id: ProcessStmtRef, ctx: &mut llhd::ir::UnitBuilder<'_> => {
    let hir = self.hir(id)?;
    let name = match hir.label {
        Some(n) => format!("{}_{}", ctx.name(), n.value),
        None => format!("{}_proc", ctx.name()),
    };
    let name = llhd::ir::UnitName::Global(name);
    debugln!("generating process `{}`", name);
    // TODO: Check which signals are actually read and written.
    // let ty = llhd::entity_ty(vec![], vec![]);
    let sig = llhd::ir::Signature::new();
    let mut prok = llhd::ir::UnitData::new(llhd::ir::UnitKind::Process, name.clone(), sig.clone());
    // let mut prok = llhd::Process::new(name, ty.clone());
    let mut prok_builder = llhd::ir::UnitBuilder::new_anonymous(&mut prok);
    // TODO: define the process as a local name
    // TOOD: codegen declarations
    // TOOD: codegen statements
    let entry_bb = prok_builder.named_block("entry");
    prok_builder.append_to(entry_bb);
    for &stmt in &hir.stmts {
        self.codegen(stmt, &mut prok_builder)?;
    }
    // TODO: codegen wait statements implied by sensitivity list

    // TODO: wire instantiation with signals in the process' port.
    let ext_unit = ctx.add_extern(
        prok_builder.name().clone(),
        prok_builder.sig().clone(),
    );
    ctx.ins().inst(ext_unit, vec![], vec![]);
    self.sb.llmod.borrow_mut().add_unit(prok);
    // ctx.add_inst(
    //     llhd::Inst::new(hir.label.map(|l| l.value.into()), llhd::InstKind::InstanceInst(
    //         ty, prok_ref.into(), vec![], vec![]
    //     )),
    //     llhd::InstPosition::End
    // );
    Ok(())
});

impl_codegen!(self, id: ConcCallStmtRef, _ctx: &mut llhd::ir::UnitBuilder<'_> => {
    unimp!(self, id);
});

impl_codegen!(self, id: ConcAssertStmtRef, _ctx: &mut llhd::ir::UnitBuilder<'_> => {
    unimp!(self, id);
});

impl_codegen!(self, id: ConcSigAssignStmtRef, _ctx: &mut llhd::ir::UnitBuilder<'_> => {
    unimp!(self, id);
});

impl_codegen!(self, id: CompInstStmtRef, _ctx: &mut llhd::ir::UnitBuilder<'_> => {
    unimp!(self, id);
});

impl_codegen!(self, id: ForGenStmtRef, _ctx: &mut llhd::ir::UnitBuilder<'_> => {
    unimp!(self, id);
});

impl_codegen!(self, id: IfGenStmtRef, _ctx: &mut llhd::ir::UnitBuilder<'_> => {
    unimp!(self, id);
});

impl_codegen!(self, id: CaseGenStmtRef, _ctx: &mut llhd::ir::UnitBuilder<'_> => {
    unimp!(self, id);
});

impl_codegen!(self, id: SeqStmtRef, _ctx: &'a mut llhd::ir::UnitBuilder<'a> => {
    unimp!(self, id);
});

impl_codegen!(self, id: SubprogDeclRef, _ctx: &mut () => {
    unimp!(self, id);
});

impl_codegen!(self, id: SubprogBodyRef, _ctx: &mut () => {
    unimp!(self, id);
});

impl_codegen!(self, id: SubprogInstRef, _ctx: &mut () => {
    unimp!(self, id);
});

impl_codegen!(self, id: PkgDeclRef, _ctx: &mut () => {
    unimp!(self, id);
});

impl_codegen!(self, id: PkgBodyRef, _ctx: &mut () => {
    unimp!(self, id);
});

impl_codegen!(self, id: PkgInstRef, _ctx: &mut () => {
    unimp!(self, id);
});

impl_codegen!(self, id: CompDeclRef, _ctx: &mut () => {
    unimp!(self, id);
});

// /// An helper to build sequences of instructions.
// pub struct InstBuilder<'ctx> {
//     pub body: &'ctx mut llhd::SeqBody,
//     pub block: llhd::BlockRef,
// }

// impl<'ctx> InstBuilder<'ctx> {
//     /// Create a new instruction builder.
//     pub fn new(body: &'ctx mut llhd::SeqBody, block: llhd::BlockRef) -> InstBuilder<'ctx> {
//         InstBuilder {
//             body: body,
//             block: block,
//         }
//     }

//     /// Add a new instruction.
//     pub fn add_inst(&mut self, inst: llhd::Inst) -> llhd::InstRef {
//         self.body
//             .add_inst(inst, llhd::InstPosition::BlockEnd(self.block))
//     }

//     /// Add a new block.
//     pub fn add_block(&mut self, block: llhd::Block) -> llhd::BlockRef {
//         self.body
//             .add_block(block, llhd::BlockPosition::After(self.block))
//     }

//     /// Change the block at the end of which instructions will be added.
//     pub fn set_block(&mut self, block: llhd::BlockRef) {
//         self.block = block
//     }
// }
