//! Redis console lines: argument splitting and which commands only read. The guard and the Redis client
//! (qurry_core::redisdb) read a line with the same rules, so a check sees the command that runs.

/// redis-cli style argument splitting: spaces, "double quotes"(\n \t \" \\ \xHH), 'single quotes'. Err = why the line can't be read
pub fn parse_command(line: &str) -> Result<Vec<Vec<u8>>, String> {
    let mut out: Vec<Vec<u8>> = Vec::new();
    let b = line.as_bytes();
    let mut i = 0;
    while i < b.len() {
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() {
            break;
        }
        let mut cur = Vec::new();
        let mut quoted = false;
        while i < b.len() && !b[i].is_ascii_whitespace() {
            match b[i] {
                b'"' => {
                    quoted = true;
                    i += 1;
                    loop {
                        if i >= b.len() {
                            return Err("Unbalanced quotes".into());
                        }
                        match b[i] {
                            b'"' => {
                                i += 1;
                                break;
                            }
                            b'\\' if i + 1 < b.len() => {
                                i += 1;
                                match b[i] {
                                    b'n' => cur.push(b'\n'),
                                    b't' => cur.push(b'\t'),
                                    b'r' => cur.push(b'\r'),
                                    b'x' if i + 2 < b.len() => {
                                        let h = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok());
                                        match h {
                                            Some(v) => {
                                                cur.push(v);
                                                i += 2;
                                            }
                                            None => cur.push(b'x'),
                                        }
                                    }
                                    c => cur.push(c),
                                }
                                i += 1;
                            }
                            c => {
                                cur.push(c);
                                i += 1;
                            }
                        }
                    }
                }
                b'\'' => {
                    quoted = true;
                    i += 1;
                    loop {
                        if i >= b.len() {
                            return Err("Unbalanced quotes".into());
                        }
                        if b[i] == b'\'' {
                            i += 1;
                            break;
                        }
                        cur.push(b[i]);
                        i += 1;
                    }
                }
                c => {
                    cur.push(c);
                    i += 1;
                }
            }
        }
        if !cur.is_empty() || quoted {
            out.push(cur);
        }
    }
    if out.is_empty() {
        return Err("Empty command".into());
    }
    Ok(out)
}

/// Whether the command doesn't change values (MCP read-only mode)
pub fn is_read_command(cmd: &str, sub: &str) -> bool {
    const READ: &[&str] = &[
        "GET", "MGET", "GETRANGE", "STRLEN", "EXISTS", "TYPE", "TTL", "PTTL", "EXPIRETIME", "PEXPIRETIME", "DBSIZE", "KEYS", "SCAN", "RANDOMKEY",
        "HGET", "HMGET", "HGETALL", "HKEYS", "HVALS", "HLEN", "HEXISTS", "HSTRLEN", "HSCAN", "HRANDFIELD",
        "LRANGE", "LLEN", "LINDEX", "LPOS", "SMEMBERS", "SCARD", "SISMEMBER", "SMISMEMBER", "SSCAN", "SRANDMEMBER", "SINTER", "SUNION", "SDIFF", "SINTERCARD",
        "ZRANGE", "ZRANGEBYSCORE", "ZRANGEBYLEX", "ZREVRANGE", "ZREVRANGEBYSCORE", "ZREVRANGEBYLEX", "ZSCORE", "ZMSCORE", "ZCARD", "ZCOUNT", "ZLEXCOUNT", "ZRANK", "ZREVRANK", "ZSCAN", "ZRANDMEMBER",
        "XRANGE", "XREVRANGE", "XLEN", "XINFO", "XPENDING", "PFCOUNT", "GETBIT", "BITCOUNT", "BITPOS", "GEOPOS", "GEODIST", "GEOSEARCH", "GEOHASH",
        "INFO", "PING", "TIME", "ECHO", "LASTSAVE", "ROLE", "LOLWUT", "DBTOOL.VIEW", "JSON.GET", "JSON.TYPE", "JSON.MGET",
    ];
    if READ.contains(&cmd) {
        return true;
    }
    match cmd {
        "MEMORY" => matches!(sub, "USAGE" | "STATS" | "DOCTOR"),
        "OBJECT" => matches!(sub, "ENCODING" | "FREQ" | "IDLETIME" | "REFCOUNT"),
        "CONFIG" => sub == "GET",
        "CLIENT" => matches!(sub, "LIST" | "INFO" | "GETNAME" | "ID"),
        "SLOWLOG" => matches!(sub, "GET" | "LEN"),
        "COMMAND" => matches!(sub, "" | "COUNT" | "INFO" | "DOCS"),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_args() {
        let a = parse_command(r#"SET "my key" 'it''s' "a\"b\n\x41" plain"#).unwrap();
        let s: Vec<String> = a.iter().map(|x| String::from_utf8_lossy(x).into_owned()).collect();
        assert_eq!(s, ["SET", "my key", "its", "a\"b\nA", "plain"]);
        assert_eq!(parse_command(r#"GET """#).unwrap()[1], b"");
        assert!(parse_command("GET \"x").is_err());
    }
}
