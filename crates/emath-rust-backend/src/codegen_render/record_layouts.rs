//! Build-local record layouts: authored objects override the installed
//! Language Image for one emission, following the `ReferenceScope`
//! thread-local pattern (the image registry is global and shared;
//! authored records are per-build and must not leak into it).

use super::*;

pub(super) type RecordFields = Vec<(String, String)>;

thread_local! {
    static AUTHORED_RECORDS: std::cell::RefCell<Vec<BTreeMap<String, RecordFields>>> =
        std::cell::RefCell::new(Vec::new());
}

pub(crate) struct AuthoredRecordScope;
impl AuthoredRecordScope {
    pub(crate) fn enter(records: BTreeMap<String, RecordFields>) -> Self {
        AUTHORED_RECORDS.with(|stack| stack.borrow_mut().push(records));
        Self
    }
}
impl Drop for AuthoredRecordScope {
    fn drop(&mut self) {
        AUTHORED_RECORDS.with(|stack| {
            stack.borrow_mut().pop();
        });
    }
}

/// Field layouts (name → carrier signature) for a record name: the
/// innermost build-local authored table first, then the installed
/// Language Image.
pub(crate) fn record_layout(name: &str) -> Option<RecordFields> {
    let authored = AUTHORED_RECORDS.with(|stack| {
        stack
            .borrow()
            .iter()
            .rev()
            .find_map(|table| table.get(name).cloned())
    });
    authored.or_else(|| {
        emath_exec_ir::native_kernel::installed_record_layout(name).map(|layout| layout.fields)
    })
}
