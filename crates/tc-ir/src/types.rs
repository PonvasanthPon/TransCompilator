//! System V AMD64 type layout rules.

use crate::ir::{Module, StructId, UnionId};

/// A Core-7 type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    /// No value.
    Void,
    /// Boolean stored as one byte.
    Bool,
    /// Signed 8-bit integer.
    I8,
    /// Signed 16-bit integer.
    I16,
    /// Signed 32-bit integer.
    I32,
    /// Signed 64-bit integer.
    I64,
    /// Unsigned 8-bit integer.
    U8,
    /// Unsigned 16-bit integer.
    U16,
    /// Unsigned 32-bit integer.
    U32,
    /// Unsigned 64-bit integer.
    U64,
    /// IEEE-754 binary32.
    F32,
    /// IEEE-754 binary64.
    F64,
    /// Pointer to another type.
    Ptr(Box<Type>),
    /// Fixed-size array.
    Array(Box<Type>, usize),
    /// Struct table reference.
    Struct(StructId),
    /// Union table reference.
    Union(UnionId),
    /// Function pointer with a concrete signature.
    FnPtr {
        /// Parameter types.
        params: Vec<Type>,
        /// Return type.
        ret: Box<Type>,
    },
}

impl Type {
    /// Returns the pointee type when this is a pointer.
    #[must_use]
    pub fn pointee(&self) -> Option<&Type> {
        if let Self::Ptr(inner) = self {
            Some(inner)
        } else {
            None
        }
    }

    /// Returns true for `Bool`.
    #[must_use]
    pub const fn is_bool(&self) -> bool {
        matches!(self, Self::Bool)
    }

    /// Returns true for signed integer primitives.
    #[must_use]
    pub const fn is_signed_int(&self) -> bool {
        matches!(self, Self::I8 | Self::I16 | Self::I32 | Self::I64)
    }

    /// Returns true for unsigned integer primitives.
    #[must_use]
    pub const fn is_unsigned_int(&self) -> bool {
        matches!(self, Self::U8 | Self::U16 | Self::U32 | Self::U64)
    }

    /// Returns true for any integer primitive.
    #[must_use]
    pub const fn is_integer(&self) -> bool {
        self.is_signed_int() || self.is_unsigned_int()
    }

    /// Returns true for `F32` and `F64`.
    #[must_use]
    pub const fn is_float(&self) -> bool {
        matches!(self, Self::F32 | Self::F64)
    }

    /// Returns true for pointers and function pointers.
    #[must_use]
    pub const fn is_pointer_like(&self) -> bool {
        matches!(self, Self::Ptr(_) | Self::FnPtr { .. })
    }

    /// Returns true for scalar values with an integer, float, or pointer representation.
    #[must_use]
    pub const fn is_scalar(&self) -> bool {
        self.is_bool() || self.is_integer() || self.is_float() || self.is_pointer_like()
    }

    /// Returns true for structs, unions, and arrays.
    #[must_use]
    pub const fn is_aggregate(&self) -> bool {
        matches!(self, Self::Array(_, _) | Self::Struct(_) | Self::Union(_))
    }

    /// Returns true when this type can be a function parameter or return value.
    #[must_use]
    pub const fn is_abi_scalar(&self) -> bool {
        self.is_scalar()
    }
}

/// Size and alignment pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    /// Object size in bytes.
    pub size: usize,
    /// Required power-of-two alignment in bytes.
    pub align: usize,
}

/// One local's location inside a function frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalSlot {
    /// Byte offset from the frame base.
    pub offset: usize,
    /// Reserved storage size (at least one byte).
    pub size: usize,
    /// Required alignment.
    pub align: usize,
}

/// The complete local layout of a function frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalLayout {
    /// One slot per parameter, followed by one slot per ordinary local.
    pub slots: Vec<LocalSlot>,
    /// End of the local area before frame rounding.
    pub size: usize,
}

const fn round_up(value: usize, align: usize) -> usize {
    value.div_ceil(align) * align
}

impl Module {
    /// Returns the byte size of a type. Recursive aggregates must be rejected
    /// by the verifier before layout is queried.
    #[must_use]
    pub fn size_of(&self, ty: &Type) -> usize {
        self.layout_of(ty).size
    }

    /// Returns the alignment of a type.
    #[must_use]
    pub fn align_of(&self, ty: &Type) -> usize {
        self.layout_of(ty).align
    }

    /// Returns the full layout of a type.
    #[must_use]
    pub fn layout_of(&self, ty: &Type) -> Layout {
        match ty {
            Type::Void => Layout { size: 0, align: 1 },
            Type::Bool | Type::I8 | Type::U8 => Layout { size: 1, align: 1 },
            Type::I16 | Type::U16 => Layout { size: 2, align: 2 },
            Type::I32 | Type::U32 | Type::F32 => Layout { size: 4, align: 4 },
            Type::I64 | Type::U64 | Type::F64 | Type::Ptr(_) | Type::FnPtr { .. } => {
                Layout { size: 8, align: 8 }
            }
            Type::Array(elem, count) => {
                let inner = self.layout_of(elem);
                Layout {
                    size: inner.size.saturating_mul(*count),
                    align: inner.align,
                }
            }
            Type::Struct(id) => self.struct_layout(*id),
            Type::Union(id) => self.union_layout(*id),
        }
    }

    fn struct_layout(&self, id: StructId) -> Layout {
        let Some(def) = self.structs.get(id.0) else {
            return Layout { size: 1, align: 1 };
        };
        let mut offset = 0usize;
        let mut align = 1usize;
        for field in &def.fields {
            let field_layout = self.layout_of(field);
            offset = round_up(offset, field_layout.align);
            offset = offset.saturating_add(field_layout.size);
            align = align.max(field_layout.align);
        }
        Layout {
            size: round_up(offset, align).max(1),
            align,
        }
    }

    fn union_layout(&self, id: UnionId) -> Layout {
        let Some(def) = self.unions.get(id.0) else {
            return Layout { size: 1, align: 1 };
        };
        let mut size = 0usize;
        let mut align = 1usize;
        for field in &def.fields {
            let field_layout = self.layout_of(field);
            size = size.max(field_layout.size);
            align = align.max(field_layout.align);
        }
        Layout {
            size: round_up(size, align).max(1),
            align,
        }
    }

    /// Returns the byte offset of a struct field.
    ///
    /// # Panics
    ///
    /// Panics if the struct or field index is out of bounds. Verified modules
    /// never trigger this.
    #[must_use]
    pub fn field_offset(&self, struct_id: StructId, field_idx: usize) -> usize {
        self.try_field_offset(struct_id, field_idx)
            .expect("verified struct field must exist")
    }

    /// Returns the byte offset of a struct field, or `None` if the identifiers
    /// are out of bounds.
    #[must_use]
    pub fn try_field_offset(&self, struct_id: StructId, field_idx: usize) -> Option<usize> {
        let def = self.structs.get(struct_id.0)?;
        let field = def.fields.get(field_idx)?;
        let mut offset = 0usize;
        for current in &def.fields[..field_idx] {
            offset = round_up(offset, self.align_of(current));
            offset = offset.saturating_add(self.size_of(current));
        }
        offset = round_up(offset, self.align_of(field));
        Some(offset)
    }

    /// Returns the byte offset of a union member (always zero).
    ///
    /// # Panics
    ///
    /// Panics if the union or field index is out of bounds. Verified modules
    /// never trigger this.
    #[must_use]
    pub fn union_field_offset(&self, union_id: UnionId, field_idx: usize) -> usize {
        self.try_union_field_offset(union_id, field_idx)
            .expect("verified union field must exist")
    }

    /// Returns the byte offset of a union member, or `None` if the identifiers
    /// are out of bounds.
    #[must_use]
    pub fn try_union_field_offset(&self, union_id: UnionId, field_idx: usize) -> Option<usize> {
        let def = self.unions.get(union_id.0)?;
        def.fields.get(field_idx)?;
        Some(0)
    }

    /// Computes aligned frame slots for all parameters and locals.
    #[must_use]
    pub fn local_layout(&self, function: &crate::ir::Function) -> LocalLayout {
        let mut slots = Vec::with_capacity(function.local_count());
        let mut cursor = 0usize;
        for local in function.local_defs() {
            let layout = self.layout_of(&local.ty);
            cursor = round_up(cursor, layout.align);
            let size = layout.size.max(1);
            slots.push(LocalSlot {
                offset: cursor,
                size,
                align: layout.align,
            });
            cursor += size;
        }
        LocalLayout {
            slots,
            size: cursor,
        }
    }

    /// Returns `true` if a type graph contains a by-value recursive aggregate.
    #[must_use]
    pub fn has_infinite_size(&self, ty: &Type) -> bool {
        let mut visiting = Vec::new();
        self.type_is_recursive(ty, &mut visiting)
    }

    fn type_is_recursive(&self, ty: &Type, visiting: &mut Vec<(u8, usize)>) -> bool {
        match ty {
            Type::Array(elem, _) => self.type_is_recursive(elem, visiting),
            Type::Struct(id) => {
                if visiting.contains(&(0, id.0)) {
                    return true;
                }
                visiting.push((0, id.0));
                let recursive = self.structs.get(id.0).is_some_and(|def| {
                    def.fields
                        .iter()
                        .any(|f| self.type_is_recursive(f, visiting))
                });
                visiting.pop();
                recursive
            }
            Type::Union(id) => {
                if visiting.contains(&(1, id.0)) {
                    return true;
                }
                visiting.push((1, id.0));
                let recursive = self.unions.get(id.0).is_some_and(|def| {
                    def.fields
                        .iter()
                        .any(|f| self.type_is_recursive(f, visiting))
                });
                visiting.pop();
                recursive
            }
            _ => false,
        }
    }
}
