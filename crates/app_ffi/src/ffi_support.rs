use std::ffi::{c_char, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};

use errors::{PlayerError, PlayerResult};
use serde::Serialize;

use crate::dto::Response;
use crate::PlayerApp;

pub(super) fn ffi_result<T, F>(operation: F) -> *mut c_char
where
    T: Serialize,
    F: FnOnce() -> PlayerResult<T>,
{
    let response = match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(Ok(data)) => Response {
            ok: true,
            data: Some(data),
            error: None,
        },
        Ok(Err(error)) => Response::<T> {
            ok: false,
            data: None,
            error: Some(error.to_string()),
        },
        Err(_) => Response::<T> {
            ok: false,
            data: None,
            error: Some("panic across FFI boundary".to_owned()),
        },
    };
    json_to_c_string(&response)
}

pub(super) fn json_to_c_string<T: Serialize>(value: &T) -> *mut c_char {
    let json = serde_json::to_string(value).unwrap_or_else(|error| {
        format!(
            r#"{{"ok":false,"data":null,"error":"serialization failed: {}"}}"#,
            error
        )
    });
    CString::new(json)
        .unwrap_or_else(|_| {
            CString::new(r#"{"ok":false,"data":null,"error":"invalid json string"}"#).unwrap()
        })
        .into_raw()
}

pub(super) unsafe fn app_mut<'a>(app: *mut PlayerApp) -> PlayerResult<&'a mut PlayerApp> {
    app.as_mut()
        .ok_or_else(|| PlayerError::engine("PlayerApp handle is null"))
}

pub(super) unsafe fn c_string(value: *const c_char) -> PlayerResult<String> {
    if value.is_null() {
        return Err(PlayerError::engine("string pointer is null"));
    }
    CStr::from_ptr(value)
        .to_str()
        .map(ToOwned::to_owned)
        .map_err(|error| PlayerError::engine(error.to_string()))
}
