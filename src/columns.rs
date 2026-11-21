//! The columns of the file views, in Python terms: the logic is in `pyegui_core::columns`, this
//! file converts what the Python code gives (type names, values, filters, keyword args) and what
//! it reads.

use std::collections::{BTreeMap, BTreeSet};

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};

pub(crate) use pyegui_core::columns::*;

pub(crate) fn parse_type(name: &str) -> PyResult<DataType> {
    DataType::parse(name).ok_or_else(|| {
        pyo3::exceptions::PyValueError::new_err("type must be \"text\", \"number\", \"date\" or \"choice\"")
    })
}

/// What a Python function returned for a column of this type. None (or a value that does not
/// fit) is Value::None: it sorts first and no filter keeps it
pub(crate) fn value_from_py(obj: &Bound<'_, PyAny>, ty: DataType) -> Value {
    if obj.is_none() {
        return Value::None;
    }
    match ty {
        DataType::Text => obj.str().map(|s| Value::Text(s.to_string())).unwrap_or(Value::None),
        DataType::Choice => match obj.extract::<bool>() {
            Ok(b) if obj.is_instance_of::<pyo3::types::PyBool>() => Value::Text(b.to_string()),
            _ => obj.str().map(|s| Value::Text(s.to_string())).unwrap_or(Value::None),
        },
        DataType::Number => obj.extract::<f64>().map(Value::Num).unwrap_or(Value::None),
        DataType::Date => obj
            .extract::<f64>()
            .or_else(|_| obj.call_method0("timestamp").and_then(|t| t.extract::<f64>()))
            .map(Value::Num)
            .unwrap_or(Value::None),
    }
}

/// A text, a (min, max) tuple or a list of values, as the Python code gives it
pub(crate) fn filter_from_py(obj: &Bound<'_, PyAny>) -> PyResult<Filter> {
    let bound = |item: &Bound<'_, PyAny>| -> PyResult<String> {
        if item.is_none() {
            Ok(String::new())
        } else if let Ok(n) = item.extract::<f64>() {
            Ok(n.to_string())
        } else {
            Ok(item.str()?.to_string())
        }
    };
    if let Ok(text) = obj.extract::<String>() {
        return Ok(Filter { text, ..Default::default() });
    }
    if let Ok(tuple) = obj.cast::<PyTuple>() {
        if tuple.len() != 2 {
            return Err(pyo3::exceptions::PyValueError::new_err("a range filter is a (min, max) tuple"));
        }
        return Ok(Filter { min: bound(&tuple.get_item(0)?)?, max: bound(&tuple.get_item(1)?)?, ..Default::default() });
    }
    let mut set = BTreeSet::new();
    for item in obj.try_iter()? {
        set.insert(item?.str()?.to_string());
    }
    Ok(Filter { choices: Some(set), ..Default::default() })
}

pub(crate) fn filter_to_py<'py>(filter: &Filter, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    if let Some(set) = &filter.choices {
        return Ok(PyList::new(py, set.iter())?.into_any());
    }
    if !filter.min.trim().is_empty() || !filter.max.trim().is_empty() {
        let side = |s: &str| -> Option<String> { (!s.trim().is_empty()).then(|| s.trim().to_string()) };
        return Ok(PyTuple::new(py, [side(&filter.min), side(&filter.max)])?.into_any());
    }
    Ok(pyo3::types::PyString::new(py, &filter.text).into_any())
}

pub(crate) fn widths_py<'py>(state: &ColumnsState, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    for (key, width) in &state.widths {
        dict.set_item(key, width)?;
    }
    Ok(dict)
}

pub(crate) fn filters_py<'py>(state: &ColumnsState, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    for (key, filter) in state.filters.iter().filter(|(_, f)| !f.is_empty()) {
        dict.set_item(key, filter_to_py(filter, py)?)?;
    }
    Ok(dict)
}

pub(crate) fn set_filters(state: &mut ColumnsState, filters: &Bound<'_, PyDict>) -> PyResult<()> {
    let mut new = BTreeMap::new();
    for (key, value) in filters.iter() {
        let filter = filter_from_py(&value)?;
        if !filter.is_empty() {
            new.insert(key.extract::<String>()?, filter);
        }
    }
    state.filters = new;
    Ok(())
}

pub(crate) fn header_options(kwargs: Option<&Bound<'_, PyDict>>, grow_left: bool) -> PyResult<HeaderOptions> {
    let flag = |name: &str| crate::kwarg::<bool>(kwargs, name).map(|v| v.unwrap_or(true));
    Ok(HeaderOptions {
        resizable: flag("resizable")?,
        pinnable: flag("pinnable")?,
        reorderable: flag("reorderable")?,
        filterable: flag("filterable")?,
        drag_out: flag("drag_out")?,
        grow_left,
    })
}
