// Shared mutable storage. Cell reads are immutable value snapshots;
// buffer handles inside those snapshots retain their shared identity.

use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

#[derive(Clone)]
pub struct CellValue {
    value: Rc<dyn Any>,
    fields: BTreeMap<String, CellValue>,
    items: Option<Vec<CellValue>>,
}

impl core::fmt::Debug for CellValue {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("CellValue")
    }
}

impl CellValue {
    pub fn new<T: Clone + 'static>(value: T) -> Self {
        Self {
            value: Rc::new(value),
            fields: BTreeMap::new(),
            items: None,
        }
    }

    pub fn with_fields(mut self, fields: BTreeMap<String, Self>) -> Self {
        self.fields = fields;
        self
    }

    pub fn with_items(mut self, items: Vec<Self>) -> Self {
        self.items = Some(items);
        self
    }

    /// Typed use, not creation, checks a heterogeneous cell's carrier.
    /// Only the existing exact numeric representation conversions apply.
    pub fn get<T: Clone + 'static>(&self) -> Result<T, String> {
        if let Some(value) = self.value.downcast_ref::<T>() {
            return Ok(value.clone());
        }
        if self.value.is::<super::code::CodeValue>()
            || self.value.is::<super::code_tree::NodeValue>()
        {
            let scalar = self.scalar()?;
            let value: Rc<dyn Any> = if TypeId::of::<T>() == TypeId::of::<bool>() {
                Rc::new(super::code::project_bool(&scalar, "buffer cell")?)
            } else if TypeId::of::<T>() == TypeId::of::<i64>() {
                Rc::new(super::code::project_i64(&scalar, "buffer cell")?)
            } else if TypeId::of::<T>() == TypeId::of::<super::ExactRatio>() {
                Rc::new(super::code::project_ratio(&scalar, "buffer cell")?)
            } else if TypeId::of::<T>() == TypeId::of::<super::ExactInt>() {
                Rc::new(super::code::project_exact_int(&scalar, "buffer cell")?)
            } else {
                self.value.clone()
            };
            return value.downcast_ref::<T>().cloned().ok_or_else(|| {
                String::from("type: buffer cell does not admit the requested carrier")
            });
        }
        let value: Rc<dyn Any> = if TypeId::of::<T>() == TypeId::of::<super::ExactRatio>() {
            if let Some(value) = self.value.downcast_ref::<i64>() {
                Rc::new((i128::from(*value), 1i128))
            } else {
                self.value.clone()
            }
        } else if TypeId::of::<T>() == TypeId::of::<super::ExactInt>() {
            if let Some(value) = self.value.downcast_ref::<i64>() {
                Rc::new(super::ExactInt::from(*value))
            } else {
                self.value.clone()
            }
        } else if TypeId::of::<T>() == TypeId::of::<i64>() {
            if let Some(value) = self.value.downcast_ref::<super::ExactInt>() {
                Rc::new(value.to_i64().ok_or_else(|| {
                    String::from("E-INT-002: buffer integer does not fit the native i64 carrier")
                })?)
            } else {
                self.value.clone()
            }
        } else {
            self.value.clone()
        };
        value
            .downcast_ref::<T>()
            .cloned()
            .ok_or_else(|| String::from("type: buffer cell does not admit the requested carrier"))
    }

    pub fn scalar(&self) -> Result<super::code::CodeValue, String> {
        use super::code::CodeValue;
        if let Some(value) = self.value.downcast_ref::<CodeValue>() {
            return Ok(*value);
        }
        if let Some(value) = self.value.downcast_ref::<super::code_tree::NodeValue>() {
            return value.to_scalar();
        }
        if let Some(value) = self.value.downcast_ref::<i64>() {
            return Ok(CodeValue::Int(*value));
        }
        if let Some(value) = self.value.downcast_ref::<super::ExactInt>() {
            return Ok(CodeValue::Int(value.to_i64().ok_or_else(|| {
                String::from("E-INT-002: buffer integer does not fit the native i64 carrier")
            })?));
        }
        if let Some(value) = self.value.downcast_ref::<bool>() {
            return Ok(CodeValue::Bool(*value));
        }
        if let Some(value) = self.value.downcast_ref::<super::ExactRatio>() {
            return Ok(CodeValue::Rat(*value));
        }
        Err(String::from(
            "type: buffer cell is not an Int/Rat/Bool scalar",
        ))
    }

    pub fn field(&self, name: &str) -> Result<Self, String> {
        if let Some(value) = self.fields.get(name) {
            return Ok(value.clone());
        }
        if name == "length" {
            if let Some(items) = &self.items {
                return Ok(Self::new(i64::try_from(items.len()).map_err(|_| {
                    String::from("overflow: sequence length exceeds the native integer carrier")
                })?));
            }
            if let Some(buffer) = self.value.downcast_ref::<Buffer>() {
                return Ok(Self::new(buffer.len()?));
            }
        }
        Err(format!("type: buffer cell has no field `{name}`"))
    }

    pub fn index(&self, index: impl Into<super::ExactInt>) -> Result<Self, String> {
        let index = index.into();
        if let Some(buffer) = self.value.downcast_ref::<Buffer>() {
            return buffer.read(index);
        }
        let items = self
            .items
            .as_ref()
            .ok_or_else(|| String::from("type: buffer cell is not an indexed carrier"))?;
        index
            .to_usize()
            .and_then(|index| items.get(index))
            .cloned()
            .ok_or_else(|| String::from("invalid_index: sequence index out of range"))
    }

    pub fn sequence(&self) -> Result<Vec<Self>, String> {
        self.items
            .clone()
            .ok_or_else(|| String::from("type: buffer cell is not a sequence"))
    }
}

#[derive(Clone)]
pub struct Buffer(Rc<RefCell<Vec<CellValue>>>);

impl core::fmt::Debug for Buffer {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("Buffer")
    }
}

impl Buffer {
    pub fn new(size: impl Into<super::ExactInt>, fill: CellValue) -> Result<Self, String> {
        let len = size.into().to_usize().ok_or_else(|| {
            String::from("invalid_index: buffer size must be a non-negative machine index")
        })?;
        if len > 4_000_000 {
            return Err(String::from(
                "overflow: buffer exceeds the carrier cell bound",
            ));
        }
        Ok(Self(Rc::new(RefCell::new(vec![fill; len]))))
    }

    pub fn len(&self) -> Result<i64, String> {
        i64::try_from(self.0.borrow().len())
            .map_err(|_| String::from("overflow: buffer length exceeds the native integer carrier"))
    }

    pub fn read(&self, index: impl Into<super::ExactInt>) -> Result<CellValue, String> {
        index
            .into()
            .to_usize()
            .and_then(|index| self.0.borrow().get(index).cloned())
            .ok_or_else(|| String::from("invalid_index: buffer index out of range"))
    }

    pub fn write(&self, index: impl Into<super::ExactInt>, value: CellValue) -> Result<(), String> {
        let index = index
            .into()
            .to_usize()
            .ok_or_else(|| String::from("invalid_index: buffer index out of range"))?;
        let mut items = self.0.borrow_mut();
        let target = items
            .get_mut(index)
            .ok_or_else(|| String::from("invalid_index: buffer index out of range"))?;
        *target = value;
        Ok(())
    }
}
