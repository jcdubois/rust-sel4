//
// Copyright 2023, Colias Group, LLC
// Copyright (c) 2020 Arm Limited
//
// SPDX-License-Identifier: MIT
//

use core::fmt;
use core::hash::Hash;
use core::marker::PhantomData;

use crate::{sys, InvocationContext, IpcBuffer, NoExplicitInvocationContext, WORD_SIZE};

/// The raw bits of a capability pointer.
pub type CPtrBits = sys::seL4_CPtr;

/// A capability pointer.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct CPtr {
    bits: CPtrBits,
}

impl CPtr {
    pub const fn bits(self) -> CPtrBits {
        self.bits
    }

    pub const fn from_bits(bits: CPtrBits) -> Self {
        Self { bits }
    }

    pub const fn cast<T: CapType>(self) -> Cap<T> {
        Cap::from_cptr(self)
    }
}

/// A capability pointer with a number of bits to resolve.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct CPtrWithDepth {
    bits: CPtrBits,
    depth: usize,
}

impl CPtrWithDepth {
    pub const fn from_bits_with_depth(bits: CPtrBits, depth: usize) -> Self {
        Self { bits, depth }
    }

    pub const fn bits(&self) -> CPtrBits {
        self.bits
    }

    pub const fn depth(&self) -> usize {
        self.depth
    }

    /// The [`CPtrWithDepth`] with a depth of 0.
    pub const fn empty() -> Self {
        Self::from_bits_with_depth(0, 0)
    }

    // convenience
    pub(crate) fn depth_for_kernel(&self) -> u8 {
        self.depth().try_into().unwrap()
    }
}

impl From<CPtr> for CPtrWithDepth {
    fn from(cptr: CPtr) -> Self {
        Self::from_bits_with_depth(cptr.bits(), WORD_SIZE)
    }
}

/// A capability pointer to be resolved in the current CSpace.
///
/// - The `T` parameter is a [`CapType`] marking the type of the pointed-to capability.
/// - The `C` parameter is a strategy for discovering the current thread's IPC buffer. When the
///   `"state"` feature is enabled, [`NoExplicitInvocationContext`] is an alias for
///   [`ImplicitInvocationContext`](crate::ImplicitInvocationContext), which uses the [`IpcBuffer`]
///   set by [`set_ipc_buffer`](crate::set_ipc_buffer). Otherwise, it is an alias for
///   [`NoInvocationContext`](crate::NoInvocationContext), which does not implement
///   [`InvocationContext`]. In such cases, the [`with`](Cap::with) method is used to specify an
///   invocation context before the capability is invoked.
///
/// The most general way to construct a [`Cap`] is with [`CPtr::cast`].
///
/// Note that `seL4_CNode_*` capability invocations are methods of [`AbsoluteCPtr`] rather than
/// [`Cap`].
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Cap<T: CapType, C = NoExplicitInvocationContext> {
    cptr: CPtr,
    invocation_context: C,
    _phantom: PhantomData<T>,
}

impl<T: CapType, C> Cap<T, C> {
    pub const fn cptr(&self) -> CPtr {
        self.cptr
    }

    pub const fn bits(&self) -> CPtrBits {
        self.cptr().bits()
    }

    pub fn cast<T1: CapType>(self) -> Cap<T1, C> {
        Cap {
            cptr: self.cptr,
            invocation_context: self.invocation_context,
            _phantom: PhantomData,
        }
    }

    pub fn with<C1>(self, context: C1) -> Cap<T, C1> {
        Cap {
            cptr: self.cptr,
            invocation_context: context,
            _phantom: PhantomData,
        }
    }

    pub fn without_context(self) -> Cap<T> {
        self.with(NoExplicitInvocationContext::new())
    }

    pub fn into_invocation_context(self) -> C {
        self.invocation_context
    }
}

impl<T: CapType> Cap<T> {
    pub const fn from_cptr(cptr: CPtr) -> Self {
        Self {
            cptr,
            invocation_context: NoExplicitInvocationContext::new(),
            _phantom: PhantomData,
        }
    }

    pub const fn from_bits(bits: CPtrBits) -> Self {
        CPtr::from_bits(bits).cast()
    }
}

impl<T: CapType, C: InvocationContext> Cap<T, C> {
    // TODO
    // Consider the tradeoffs of taking &mut self here, and switching all object invocations to take
    // &mut self too.
    pub(crate) fn invoke<R>(self, f: impl FnOnce(CPtr, &mut IpcBuffer) -> R) -> R {
        let cptr = self.cptr();
        self.into_invocation_context()
            .with_context(|ipc_buffer| f(cptr, ipc_buffer))
    }
}

impl<T: CapType, C> fmt::Debug for Cap<T, C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple(T::NAME).field(&self.cptr().bits()).finish()
    }
}

/// Trait for marker types corresponding to capability types in the seL4 API.
///
/// Implementors are used to mark instantiations of [`Cap`].
// NOTE require derivable traits for convenience to make up for limitations of automatic trait
// derivation
pub trait CapType: Copy + Clone + Eq + PartialEq + Ord + PartialOrd + Hash {
    const NAME: &'static str;
}

pub mod cap_type {
    //! Markers corresponding to capability types and classes of capability types.
    //!
    //! These types are used for marking [`Cap`](crate::Cap).

    use sel4_config::sel4_cfg_if;

    use crate::{
        declare_cap_type, declare_cap_type_for_object_of_fixed_size,
        declare_cap_type_for_object_of_variable_size,
    };

    pub use crate::arch::cap_type_arch::*;

    declare_cap_type_for_object_of_variable_size! {
        /// Corresponds to `seL4_Untyped`.
        Untyped { ObjectType, ObjectBlueprint }
    }

    declare_cap_type_for_object_of_fixed_size! {
        /// Corresponds to the endpoint capability type.
        Endpoint { ObjectType, ObjectBlueprint }
    }

    declare_cap_type_for_object_of_fixed_size! {
        /// Corresponds to the notification capability type.
        Notification { ObjectType, ObjectBlueprint }
    }

    declare_cap_type_for_object_of_fixed_size! {
        /// Corresponds to `seL4_TCB`.
        Tcb { ObjectType, ObjectBlueprint }
    }

    declare_cap_type_for_object_of_variable_size! {
        /// Corresponds to `seL4_CNode`.
        CNode { ObjectType, ObjectBlueprint }
    }

    declare_cap_type! {
        /// Corresponds to `seL4_IRQControl`.
        IrqControl
    }

    declare_cap_type! {
        /// Corresponds to `seL4_IRQHandler`.
        IrqHandler
    }

    declare_cap_type! {
        /// Corresponds to `seL4_ASIDControl`.
        AsidControl
    }

    declare_cap_type! {
        /// Corresponds to `seL4_ASIDPool`.
        AsidPool
    }

    declare_cap_type! {
        /// Corresponds to the null capability.
        Null
    }

    declare_cap_type! {
        /// Any capability.
        Unspecified
    }

    declare_cap_type! {
        /// Any page capability.
        UnspecifiedPage
    }

    declare_cap_type! {
        /// Any intermediate translation table capability.
        UnspecifiedIntermediateTranslationTable
    }

    sel4_cfg_if! {
        if #[sel4_cfg(KERNEL_MCS)] {
            declare_cap_type! {
                /// Corresponds to the reply capability type (MCS only).
                Reply
            }

            declare_cap_type_for_object_of_variable_size! {
                /// Corresponds to the scheduling context capability type (MCS only).
                SchedContext { ObjectType, ObjectBlueprint }
            }

            declare_cap_type! {
                /// Corresponds to `seL4_SchedControl`.
                SchedControl
            }
        }
    }
}

use cap::*;

pub mod cap {
    //! Marked aliases of [`Cap`](crate::Cap).
    //!
    //! Each type `$t<C = NoExplicitInvocationContext>` in this module is an alias for `Cap<$t, C>`.

    use sel4_config::sel4_cfg_if;

    use crate::declare_cap_alias;

    pub use crate::arch::cap_arch::*;

    declare_cap_alias!(Untyped);
    declare_cap_alias!(Endpoint);
    declare_cap_alias!(Notification);
    declare_cap_alias!(Tcb);
    declare_cap_alias!(CNode);
    declare_cap_alias!(IrqControl);
    declare_cap_alias!(IrqHandler);
    declare_cap_alias!(AsidControl);
    declare_cap_alias!(AsidPool);

    declare_cap_alias!(Null);
    declare_cap_alias!(Unspecified);
    declare_cap_alias!(UnspecifiedPage);
    declare_cap_alias!(UnspecifiedIntermediateTranslationTable);

    declare_cap_alias!(VSpace);
    declare_cap_alias!(Granule);

    sel4_cfg_if! {
        if #[sel4_cfg(KERNEL_MCS)] {
            declare_cap_alias!(Reply);
            declare_cap_alias!(SchedContext);
            declare_cap_alias!(SchedControl);
        }
    }
}

impl<T: CapType, C> Cap<T, C> {
    pub fn upcast(self) -> Unspecified<C> {
        self.cast()
    }
}

impl<C> Unspecified<C> {
    pub fn downcast<T: CapType>(self) -> Cap<T, C> {
        self.cast()
    }
}

/// A [`CPtrWithDepth`] to be resolved in the context of a particular [`CNode`].
///
/// [`AbsoluteCPtr`] addresses capability slots in a more general way than [`Cap`]. It allows one to
/// address any capability slot that is directly addressable from any CNode that is directly
/// addressible in the current thread's CSpace. Furthermore, it allows one to address capability
/// slots that contain CNodes by limiting the lookup depth to prevent the kernel's lookup procedure
/// from descending into the CNode contained in that slot.
///
/// `seL4_CNode_*` capability invocations are methods of [`AbsoluteCPtr`] rather than [`Cap`].
///
/// In addition to [`AbsoluteCPtr::new`], the following methods can be used to construct an
/// [`AbsoluteCPtr`]:
/// - [`CNode::absolute_cptr`]
/// - [`CNode::absolute_cptr_from_bits_with_depth`]
/// - [`CNode::absolute_cptr_for_self`]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct AbsoluteCPtr<C = NoExplicitInvocationContext> {
    root: CNode<C>,
    path: CPtrWithDepth,
}

impl<C> AbsoluteCPtr<C> {
    pub const fn new(root: CNode<C>, path: CPtrWithDepth) -> Self {
        Self { root, path }
    }

    pub const fn root(&self) -> &CNode<C> {
        &self.root
    }

    pub fn into_root(self) -> CNode<C> {
        self.root
    }

    pub const fn path(&self) -> &CPtrWithDepth {
        &self.path
    }

    pub fn with<C1>(self, context: C1) -> AbsoluteCPtr<C1> {
        AbsoluteCPtr {
            root: self.root.with(context),
            path: self.path,
        }
    }

    pub fn without_context(self) -> AbsoluteCPtr {
        self.with(NoExplicitInvocationContext::new())
    }
}

impl<C: InvocationContext> AbsoluteCPtr<C> {
    pub(crate) fn invoke<R>(self, f: impl FnOnce(CPtr, CPtrWithDepth, &mut IpcBuffer) -> R) -> R {
        let path = *self.path();
        self.into_root()
            .invoke(|cptr, ipc_buffer| f(cptr, path, ipc_buffer))
    }
}

/// Trait for types whose members which logically contain a [`CPtrWithDepth`].
///
/// [`CPtr`] and [`Cap`] each logically contain a [`CPtrWithDepth`] with a depth of [`WORD_SIZE`].
pub trait HasCPtrWithDepth {
    /// Returns the logical [`CPtrWithDepth`] entailed by `self`.
    fn cptr_with_depth(self) -> CPtrWithDepth;
}

impl HasCPtrWithDepth for CPtr {
    fn cptr_with_depth(self) -> CPtrWithDepth {
        self.into()
    }
}

impl<T: CapType, C> HasCPtrWithDepth for Cap<T, C> {
    fn cptr_with_depth(self) -> CPtrWithDepth {
        self.cptr().into()
    }
}

impl HasCPtrWithDepth for CPtrWithDepth {
    fn cptr_with_depth(self) -> CPtrWithDepth {
        self
    }
}

impl<C> CNode<C> {
    /// Returns the [`AbsoluteCPtr`] for `path` in the context of `self`.
    pub fn absolute_cptr<T: HasCPtrWithDepth>(self, path: T) -> AbsoluteCPtr<C> {
        AbsoluteCPtr {
            root: self,
            path: path.cptr_with_depth(),
        }
    }

    /// Returns the [`AbsoluteCPtr`] for
    /// [`CPtrWithDepth::from_bits_with_depth(bits, depth)`](CPtrWithDepth::from_bits_with_depth)
    /// in the context of `self`.
    pub fn absolute_cptr_from_bits_with_depth(
        self,
        bits: CPtrBits,
        depth: usize,
    ) -> AbsoluteCPtr<C> {
        self.absolute_cptr(CPtrWithDepth::from_bits_with_depth(bits, depth))
    }

    /// Returns the [`AbsoluteCPtr`] for `self` in its own context.
    ///
    /// Currently implemented as:
    /// ```rust
    /// self.absolute_cptr(CPtrWithDepth::empty())
    /// ```
    pub fn absolute_cptr_for_self(self) -> AbsoluteCPtr<C> {
        self.absolute_cptr(CPtrWithDepth::empty())
    }
}
