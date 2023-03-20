use crate::Database;
use std::any::Any;
use std::marker::PhantomData;
use std::rc::Rc;

pub trait Input {}
impl<T> Input for T {}

struct ConstrainedFn<A: Input, O, F: Fn(&Database, A) -> O> {
    pub f: F,
    pub _arg: PhantomData<A>,
    pub _out: PhantomData<O>,
}

pub trait AnyFn {
    /// Only safe to call if arg points to the argument that this function expects
    unsafe fn try_call<'db, 'input>(
        &self,
        db: &'db Database,
        arg: *const (dyn Input + 'input),
    ) -> Rc<dyn Any>;
}

impl<A, O: 'static, F: Fn(&Database, A) -> O + 'static> AnyFn for ConstrainedFn<A, O, F> {
    unsafe fn try_call<'db, 'input>(
        &self,
        db: &'db Database,
        arg: *const (dyn Input + 'input),
    ) -> Rc<dyn Any> {
        let arg = unsafe {
            let arg = arg.cast::<A>();
            std::ptr::read(arg)
        };

        Rc::new((self.f)(db, arg))
    }
}

/// Erases the static requirement of the input type
pub fn into_erased<'input, I: 'input, O, F>(f: Box<F>) -> Box<dyn AnyFn>
where
    F: Fn(&Database, I) -> O + 'static,
    O: 'static,
{
    let constrained = ConstrainedFn {
        f,
        _arg: Default::default(),
        _out: Default::default(),
    };

    let boxed = Box::new(constrained) as Box<dyn AnyFn>;

    unsafe { std::mem::transmute::<Box<dyn AnyFn + 'input>, Box<dyn AnyFn + 'static>>(boxed) }
}
