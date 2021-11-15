// Copyright (c) 2016-2021 Fabian Schuiki

//! Facilities to deal with MLIR attributes.

use crate::crate_prelude::*;
use std::fmt::{Debug, Display};

/// An MLIR attribute.
#[derive(Clone, Copy)]
pub struct Attribute(MlirAttribute);

impl Attribute {
    /// Get the attribute's MLIR context.
    pub fn context(&self) -> Context {
        Context::from_raw(unsafe { mlirAttributeGetContext(self.raw()) })
    }

    /// Associate a name with this attribute.
    pub fn named(&self, name: &str) -> MlirNamedAttribute {
        unsafe { mlirNamedAttributeGet(self.context().get_identifier(name), self.raw()) }
    }
}

impl WrapRaw for Attribute {
    type RawType = MlirAttribute;
    fn from_raw(raw: MlirAttribute) -> Self {
        Self(raw)
    }
    fn raw(&self) -> MlirAttribute {
        self.0
    }
}

impl Display for Attribute {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        unsafe {
            mlirAttributePrint(
                self.raw(),
                Some(super::print_formatter_callback),
                f as *const _ as *mut _,
            )
        };
        Ok(())
    }
}

impl Debug for Attribute {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

/// Create a new integer attribute.
pub fn get_integer_attr(ty: Type, value: i64) -> Attribute {
    Attribute::from_raw(unsafe { mlirIntegerAttrGet(ty.raw(), value) })
}

/// Create a new string attribute.
pub fn get_string_attr(cx: Context, value: &str) -> Attribute {
    Attribute::from_raw(unsafe { mlirStringAttrGet(cx.raw(), mlirStringRefCreateFromStr(value)) })
}
