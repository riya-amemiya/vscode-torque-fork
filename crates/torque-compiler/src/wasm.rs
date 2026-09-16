// Copyright 2026 Riya Amemiya.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::cell::RefCell;

use crate::compile::compile_json;

thread_local! {
    static RESULT: RefCell<Vec<u8>> = RefCell::new(Vec::new());
}

#[unsafe(no_mangle)]
pub extern "C" fn torque_alloc(size: u32) -> *mut u8 {
    let mut buffer = vec![0u8; size as usize];
    let ptr = buffer.as_mut_ptr();
    std::mem::forget(buffer);
    ptr
}

#[unsafe(no_mangle)]
pub extern "C" fn torque_free(ptr: *mut u8, size: u32) {
    if ptr.is_null() {
        return;
    }
    unsafe {
        drop(Vec::from_raw_parts(ptr, size as usize, size as usize));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn torque_compile(ptr: *const u8, len: u32) -> *const u8 {
    let input = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    let text = std::str::from_utf8(input).unwrap_or("");
    let output = compile_json(text);
    RESULT.with(|result| {
        let mut result = result.borrow_mut();
        result.clear();
        result.extend_from_slice(output.as_bytes());
        result.as_ptr()
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn torque_result_len() -> u32 {
    RESULT.with(|result| result.borrow().len() as u32)
}
