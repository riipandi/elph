//! Length-prefixed UTF-8 blobs in guest linear memory.

use anyhow::{Context, Result, bail, ensure};
use wasmi::{AsContext, AsContextMut, Caller, Memory};

pub const MAX_BLOB: usize = 64 * 1024;

pub fn memory_from_caller<T>(caller: &Caller<'_, T>) -> Result<Memory> {
    caller
        .get_export("memory")
        .and_then(|export| export.into_memory())
        .context("guest did not export memory")
}

pub fn read_blob<T>(caller: &Caller<'_, T>, ptr: i32, len: i32) -> Result<Vec<u8>> {
    ensure!(ptr >= 0 && len >= 0, "negative pointer or length");
    let len = len as usize;
    ensure!(len <= MAX_BLOB, "blob exceeds {MAX_BLOB} bytes");
    let memory = memory_from_caller(caller)?;
    let mut buf = vec![0u8; len];
    memory
        .read(caller, ptr as usize, &mut buf)
        .map_err(|error| anyhow::anyhow!("read guest memory: {error}"))?;
    Ok(buf)
}

pub fn read_utf8<T>(caller: &Caller<'_, T>, ptr: i32, len: i32) -> Result<String> {
    let bytes = read_blob(caller, ptr, len)?;
    String::from_utf8(bytes).context("guest string is not UTF-8")
}

pub fn read_len_prefixed<C: AsContext>(store: C, memory: Memory, ptr: i32) -> Result<Vec<u8>> {
    ensure!(ptr > 0, "null guest pointer");
    let mut header = [0u8; 4];
    memory
        .read(&store, ptr as usize, &mut header)
        .map_err(|error| anyhow::anyhow!("read length prefix: {error}"))?;
    let len = u32::from_le_bytes(header) as usize;
    ensure!(len <= MAX_BLOB, "blob exceeds {MAX_BLOB} bytes");
    let mut buf = vec![0u8; len];
    memory
        .read(&store, ptr as usize + 4, &mut buf)
        .map_err(|error| anyhow::anyhow!("read payload: {error}"))?;
    Ok(buf)
}

pub fn write_guest_bytes<T>(
    store: &mut impl AsContextMut<Data = T>,
    memory: Memory,
    alloc: &wasmi::TypedFunc<i32, i32>,
    bytes: &[u8],
) -> Result<i32> {
    ensure!(bytes.len() <= MAX_BLOB, "blob exceeds {MAX_BLOB} bytes");
    let ptr = alloc
        .call(&mut *store, bytes.len() as i32)
        .map_err(|error| anyhow::anyhow!("elph_alloc: {error}"))?;
    if ptr == 0 {
        bail!("elph_alloc returned null");
    }
    memory
        .write(&mut *store, ptr as usize, bytes)
        .map_err(|error| anyhow::anyhow!("write guest memory: {error}"))?;
    Ok(ptr)
}
