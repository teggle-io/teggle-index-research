use alloc::string::{String, ToString};
use alloc::vec::Vec;

use bytes::BytesMut;
use http::{header::HeaderValue, Request, Response};
use http::request::Builder;
use lazy_static::lazy_static;
use std::fmt;

use crate::api::results::{Error, ErrorKind};

lazy_static! {
    pub(crate) static ref GLOBAL_CODEC: HttpCodec = {
        HttpCodec::new("index.teggle.io/v1beta1")
    };
}

// Borrowed from: https://github.com/tokio-rs/tokio/blob/master/examples/tinyhttp.rs

pub(crate) struct HttpCodec {
    server: String,
}

impl HttpCodec {
    pub(crate) fn new(server: &str) -> Self {
        Self { server: server.to_string() }
    }

    pub(crate) fn encode(
        &self,
        item: Response<()>,
        dst: &mut BytesMut,
        content_length: usize
    ) -> Result<(), Error> {
        use std::fmt::Write;

        write!(
            BytesWrite(dst),
            "\
             {:?} {}\r\n\
             Server: {}\r\n\
             Content-Length: {}\r\n\
             Date: {}\r\n\
             ",
            item.version(),
            item.status(),
            self.server,
            content_length,
            date::now()
        ).map_err(|e| {
            Error::new_with_kind(ErrorKind::EncodeFault, e.to_string())
        })?;

        for (k, v) in item.headers() {
            dst.extend_from_slice(k.as_str().as_bytes());
            dst.extend_from_slice(b": ");
            dst.extend_from_slice(v.as_bytes());
            dst.extend_from_slice(b"\r\n");
        }

        dst.extend_from_slice(b"\r\n");

        Ok(())
    }

    pub(crate) fn decode(&self, src: &mut BytesMut) -> Result<Option<Builder>, Error> {
        // TODO: we should grow this headers array if parsing fails and asks
        //       for more headers
        let mut headers = [None; 16];
        let (method, path, version, amt) = {
            let mut parsed_headers = [httparse::EMPTY_HEADER; 16];
            let mut r = httparse::Request::new(&mut parsed_headers);
            let status = r.parse(src).map_err(|e| {
                return Error::new_with_kind(ErrorKind::DecodeFault,
                                            format!("failed to parse http request: {:?}", e));
            })?;

            let amt = match status {
                httparse::Status::Complete(amt) => amt,
                httparse::Status::Partial => return Ok(None),
            };

            let toslice = |a: &[u8]| {
                let start = a.as_ptr() as usize - src.as_ptr() as usize;
                assert!(start < src.len());
                (start, start + a.len())
            };

            for (i, header) in r.headers.iter().enumerate() {
                let k = toslice(header.name.as_bytes());
                let v = toslice(header.value);
                headers[i] = Some((k, v));
            }

            (
                toslice(r.method.unwrap().as_bytes()),
                toslice(r.path.unwrap().as_bytes()),
                r.version.unwrap(),
                amt,
            )
        };

        let data = src.split_to(amt).freeze();
        let mut ret = Request::builder();
        ret = ret.method(&data[method.0..method.1]);
        let s = data.slice(path.0..path.1);
        let s = unsafe { String::from_utf8_unchecked(Vec::from(s.as_ref())) };
        ret = ret.uri(s);

        match version {
            0 => { ret = ret.version(http::Version::HTTP_10); },
            1 => { ret = ret.version(http::Version::HTTP_11); },
            _ => {
                return Err(Error::new_with_kind(
                    ErrorKind::DecodeFault,
                    "only HTTP/1.0 or 1.1 accepted".to_string(),
                ));
            }
        }

        for header in headers.iter() {
            let (k, v) = match *header {
                Some((ref k, ref v)) => (k, v),
                None => break,
            };
            let value = HeaderValue::from_bytes(data.slice(v.0..v.1).as_ref())
                .map_err(|_| Error::new_with_kind(ErrorKind::DecodeFault,
                                                  "header decode error".to_string()))?;
            ret = ret.header(&data[k.0..k.1], value);
        }

        Ok(Some(ret))
    }
}

// Right now `write!` on `Vec<u8>` goes through io::Write and is not
// super speedy, so inline a less-crufty implementation here which
// doesn't go through io::Error.
struct BytesWrite<'a>(&'a mut BytesMut);

impl fmt::Write for BytesWrite<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.0.extend_from_slice(s.as_bytes());
        Ok(())
    }

    fn write_fmt(&mut self, args: fmt::Arguments<'_>) -> fmt::Result {
        fmt::write(self, args)
    }
}


mod date {
    use std::cell::RefCell;
    use std::fmt::{self, Write};
    use std::str;
    use std::time::SystemTime;

    use httpdate::HttpDate;

    pub struct Now(());

    /// Returns a struct, which when formatted, renders an appropriate `Date`
    /// header value.
    pub fn now() -> Now {
        Now(())
    }

    // Gee Alex, doesn't this seem like premature optimization. Well you see
    // there Billy, you're absolutely correct! If your server is *bottlenecked*
    // on rendering the `Date` header, well then boy do I have news for you, you
    // don't need this optimization.
    //
    // In all seriousness, though, a simple "hello world" benchmark which just
    // sends back literally "hello world" with standard headers actually is
    // bottlenecked on rendering a date into a byte buffer. Since it was at the
    // top of a profile, and this was done for some competitive benchmarks, this
    // module was written.
    //
    // Just to be clear, though, I was not intending on doing this because it
    // really does seem kinda absurd, but it was done by someone else [1], so I
    // blame them!  :)
    //
    // [1]: https://github.com/rapidoid/rapidoid/blob/f1c55c0555007e986b5d069fe1086e6d09933f7b/rapidoid-commons/src/main/java/org/rapidoid/commons/Dates.java#L48-L66

    struct LastRenderedNow {
        bytes: [u8; 128],
        amt: usize,
        unix_date: u64,
    }

    thread_local!(static LAST: RefCell<LastRenderedNow> = RefCell::new(LastRenderedNow {
        bytes: [0; 128],
        amt: 0,
        unix_date: 0,
    }));

    impl fmt::Display for Now {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            LAST.with(|cache| {
                let mut cache = cache.borrow_mut();
                let now = SystemTime::now();
                let now_unix = now
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .map(|since_epoch| since_epoch.as_secs())
                    .unwrap_or(0);
                if cache.unix_date != now_unix {
                    cache.update(now, now_unix);
                }
                f.write_str(cache.buffer())
            })
        }
    }

    impl LastRenderedNow {
        fn buffer(&self) -> &str {
            str::from_utf8(&self.bytes[..self.amt]).unwrap()
        }

        fn update(&mut self, now: SystemTime, now_unix: u64) {
            self.amt = 0;
            self.unix_date = now_unix;
            write!(LocalBuffer(self), "{}", HttpDate::from(now)).unwrap();
        }
    }

    struct LocalBuffer<'a>(&'a mut LastRenderedNow);

    impl fmt::Write for LocalBuffer<'_> {
        fn write_str(&mut self, s: &str) -> fmt::Result {
            let start = self.0.amt;
            let end = start + s.len();
            self.0.bytes[start..end].copy_from_slice(s.as_bytes());
            self.0.amt += s.len();
            Ok(())
        }
    }
}
