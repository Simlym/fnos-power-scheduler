use crate::Result;
use std::collections::BTreeMap;
use std::io::{Read, Write};
pub struct Request {
    pub method: String,
    pub path: String,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}
pub fn read_http(stream: &mut impl Read) -> Result<Request> {
    let mut bytes = Vec::new();
    loop {
        if bytes.len() >= 16384 {
            return Err("请求头过大".into());
        }
        let mut b = [0];
        stream.read_exact(&mut b)?;
        bytes.push(b[0]);
        if bytes.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    let mut headers = [httparse::EMPTY_HEADER; 32];
    let mut req = httparse::Request::new(&mut headers);
    req.parse(&bytes)?;
    let method = req.method.ok_or("缺少方法")?.to_string();
    let path = req.path.ok_or("缺少路径")?.to_string();
    let mut fields = BTreeMap::new();
    for h in req.headers.iter() {
        let key = h.name.to_ascii_lowercase();
        if fields
            .insert(key, std::str::from_utf8(h.value)?.to_string())
            .is_some()
        {
            return Err("重复请求头".into());
        }
    }
    if fields.contains_key("transfer-encoding") {
        return Err("不支持分块请求".into());
    }
    let length = fields
        .get("content-length")
        .map(|s| s.parse::<usize>())
        .transpose()?
        .unwrap_or(0);
    if length > 16384 {
        return Err("请求体过大".into());
    }
    let mut body = vec![0; length];
    stream.read_exact(&mut body)?;
    Ok(Request {
        method,
        path,
        headers: fields,
        body,
    })
}
pub fn respond(stream: &mut impl Write, status: u16, mime: &str, body: &[u8]) -> Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "Internal Server Error",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    Ok(())
}
#[cfg(target_os = "linux")]
pub fn peer_uid(stream: &std::os::unix::net::UnixStream) -> Result<u32> {
    use std::os::fd::AsRawFd;
    let mut cred = std::mem::MaybeUninit::<libc::ucred>::uninit();
    let mut size = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SO_PEERCRED initializes exactly the credential struct on successful Linux calls.
    if unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            cred.as_mut_ptr().cast(),
            &mut size,
        )
    } != 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    if size as usize != std::mem::size_of::<libc::ucred>() {
        return Err("无效对端凭据".into());
    }
    Ok(unsafe { cred.assume_init() }.uid)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_framing() {
        for bytes in [
            b"POST / HTTP/1.1\r\nContent-Length: 1\r\nContent-Length: 2\r\n\r\na".as_slice(),
            b"POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n",
            b"POST / HTTP/1.1\r\nContent-Length: 16385\r\n\r\n",
            b"POST / HTTP/1.1\r\nContent-Length: 3\r\n\r\na",
        ] {
            assert!(read_http(&mut std::io::Cursor::new(bytes)).is_err());
        }
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn kernel_peer_credentials() {
        let (a, _) = std::os::unix::net::UnixStream::pair().unwrap();
        assert_eq!(peer_uid(&a).unwrap(), unsafe { libc::geteuid() });
    }
}
