//! Redis console lines: argument splitting and which commands only read. The guard and the Redis client
//! (qurry_core::redisdb) read a line with the same rules, so a check sees the command that runs.
//! Also the Terminal's bundled command table (completion, argument hints, HELP) and how it shows replies, for the GUI (WASM) and the TUI.

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

/// Commands that only read, whatever their arguments (is_read_command decides the container commands by their subcommand)
const READ: &[&str] = &[
    "GET", "MGET", "GETRANGE", "STRLEN", "EXISTS", "TYPE", "TTL", "PTTL", "EXPIRETIME", "PEXPIRETIME", "DBSIZE", "KEYS", "SCAN", "RANDOMKEY",
    "HGET", "HMGET", "HGETALL", "HKEYS", "HVALS", "HLEN", "HEXISTS", "HSTRLEN", "HSCAN", "HRANDFIELD",
    "LRANGE", "LLEN", "LINDEX", "LPOS", "SMEMBERS", "SCARD", "SISMEMBER", "SMISMEMBER", "SSCAN", "SRANDMEMBER", "SINTER", "SUNION", "SDIFF", "SINTERCARD",
    "ZRANGE", "ZRANGEBYSCORE", "ZRANGEBYLEX", "ZREVRANGE", "ZREVRANGEBYSCORE", "ZREVRANGEBYLEX", "ZSCORE", "ZMSCORE", "ZCARD", "ZCOUNT", "ZLEXCOUNT", "ZRANK", "ZREVRANK", "ZSCAN", "ZRANDMEMBER",
    "XRANGE", "XREVRANGE", "XLEN", "XINFO", "XPENDING", "PFCOUNT", "GETBIT", "BITCOUNT", "BITPOS", "GEOPOS", "GEODIST", "GEOSEARCH", "GEOHASH",
    "INFO", "PING", "TIME", "ECHO", "LASTSAVE", "ROLE", "LOLWUT", "DBTOOL.VIEW", "JSON.GET", "JSON.TYPE", "JSON.MGET",
];

/// Whether the command doesn't change values (read-only data sources, MCP read-only mode, the Production question, the Terminal's
/// command table). Anything not listed, an unknown command included, is a write
pub fn is_read_command(cmd: &str, sub: &str) -> bool {
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

// ───────── Terminal: the bundled command table, completion, HELP, reply shapes ─────────

/// One command of the Terminal's table: its name (a container command's subcommand as "CONFIG GET"), its arguments as Redis
/// documents them ("key" marks a key name, completed from SCAN) and a one-line summary. Whether it reads comes from
/// is_read_command, so the table and the guard can't disagree
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct CommandDoc {
    pub name: &'static str,
    pub args: &'static str,
    pub summary: &'static str,
}

impl CommandDoc {
    /// Read or write, as the guard decides (a read-only data source runs only reads)
    pub fn read(&self) -> bool {
        let mut w = self.name.split(' ');
        is_read_command(w.next().unwrap_or(""), w.next().unwrap_or(""))
    }
    /// The arguments up to the first optional one
    fn positional(&self) -> Vec<&'static str> {
        self.args.split_whitespace().take_while(|t| !t.starts_with('[')).collect()
    }
}

const fn d(name: &'static str, args: &'static str, summary: &'static str) -> CommandDoc {
    CommandDoc { name, args, summary }
}

/// Sorted by name (completion lists them in this order)
pub const COMMANDS: &[CommandDoc] = &[
    d("APPEND", "key value", "Appends a string to the value of a key"),
    d("AUTH", "[username] password", "Authenticates the connection"),
    d("BITCOUNT", "key [start end [BYTE | BIT]]", "Counts the set bits in a string"),
    d("BITPOS", "key bit [start [end [BYTE | BIT]]]", "Finds the first set or clear bit in a string"),
    d("BLMOVE", "source destination LEFT | RIGHT LEFT | RIGHT timeout", "Moves an element between lists, waiting for one"),
    d("BLPOP", "key [key ...] timeout", "Removes and returns the first element of a list, waiting for one"),
    d("BRPOP", "key [key ...] timeout", "Removes and returns the last element of a list, waiting for one"),
    d("BZPOPMAX", "key [key ...] timeout", "Removes and returns the highest-scored member, waiting for one"),
    d("BZPOPMIN", "key [key ...] timeout", "Removes and returns the lowest-scored member, waiting for one"),
    d("CLIENT GETNAME", "", "Returns the name of the connection"),
    d("CLIENT ID", "", "Returns the id of the connection"),
    d("CLIENT INFO", "", "Returns information about the connection"),
    d("CLIENT KILL", "[ID client-id] [ADDR ip:port] [USER username]", "Closes client connections"),
    d("CLIENT LIST", "[TYPE NORMAL | MASTER | REPLICA | PUBSUB]", "Lists the client connections"),
    d("CLIENT SETNAME", "connection-name", "Sets the name of the connection"),
    d("COMMAND COUNT", "", "Returns the number of commands"),
    d("COMMAND DOCS", "[command-name ...]", "Returns the documentation of commands"),
    d("COMMAND INFO", "[command-name ...]", "Returns information about commands"),
    d("CONFIG GET", "parameter [parameter ...]", "Returns the values of configuration parameters"),
    d("CONFIG RESETSTAT", "", "Resets the server's statistics"),
    d("CONFIG REWRITE", "", "Rewrites the configuration file with the running configuration"),
    d("CONFIG SET", "parameter value [parameter value ...]", "Sets configuration parameters"),
    d("COPY", "source destination [DB destination-db] [REPLACE]", "Copies the value of a key to a new key"),
    d("DBSIZE", "", "Returns the number of keys in the database"),
    d("DECR", "key", "Decrements the integer value of a key by one"),
    d("DECRBY", "key decrement", "Decrements the integer value of a key by a number"),
    d("DEL", "key [key ...]", "Deletes keys"),
    d("DUMP", "key", "Returns a serialized representation of the value of a key"),
    d("ECHO", "message", "Returns the given string"),
    d("EVAL", "script numkeys [key ...] [arg ...]", "Runs a Lua script"),
    d("EVALSHA", "sha1 numkeys [key ...] [arg ...]", "Runs a cached Lua script"),
    d("EXISTS", "key [key ...]", "Counts how many of the keys exist"),
    d("EXPIRE", "key seconds [NX | XX | GT | LT]", "Sets the expiry of a key in seconds"),
    d("EXPIREAT", "key unix-time-seconds [NX | XX | GT | LT]", "Sets the expiry of a key to a Unix time"),
    d("EXPIRETIME", "key", "Returns the expiry of a key as a Unix time"),
    d("FLUSHALL", "[ASYNC | SYNC]", "Deletes every key of every database"),
    d("FLUSHDB", "[ASYNC | SYNC]", "Deletes every key of the current database"),
    d("GEOADD", "key [NX | XX] [CH] longitude latitude member [longitude latitude member ...]", "Adds members with coordinates to a geospatial index"),
    d("GEODIST", "key member1 member2 [M | KM | FT | MI]", "Returns the distance between two members"),
    d("GEOHASH", "key [member ...]", "Returns members as geohash strings"),
    d("GEOPOS", "key [member ...]", "Returns the coordinates of members"),
    d("GEOSEARCH", "key FROMMEMBER member | FROMLONLAT longitude latitude BYRADIUS radius M | KM | FT | MI", "Finds members inside an area"),
    d("GET", "key", "Returns the string value of a key"),
    d("GETBIT", "key offset", "Returns a bit of a string"),
    d("GETDEL", "key", "Returns the string value of a key and deletes the key"),
    d("GETEX", "key [EX seconds | PX milliseconds | EXAT unix-time-seconds | PXAT unix-time-milliseconds | PERSIST]", "Returns the string value of a key and changes its expiry"),
    d("GETRANGE", "key start end", "Returns a substring of a string"),
    d("GETSET", "key value", "Sets a string and returns the old value"),
    d("HDEL", "key field [field ...]", "Deletes fields of a hash"),
    d("HELLO", "[protover [AUTH username password] [SETNAME clientname]]", "Handshakes with the server"),
    d("HEXISTS", "key field", "Whether a field exists in a hash"),
    d("HGET", "key field", "Returns the value of a field in a hash"),
    d("HGETALL", "key", "Returns every field and value of a hash"),
    d("HINCRBY", "key field increment", "Increments the integer value of a hash field"),
    d("HINCRBYFLOAT", "key field increment", "Increments the float value of a hash field"),
    d("HKEYS", "key", "Returns every field of a hash"),
    d("HLEN", "key", "Returns the number of fields in a hash"),
    d("HMGET", "key field [field ...]", "Returns the values of fields in a hash"),
    d("HMSET", "key field value [field value ...]", "Sets fields of a hash (HSET does the same)"),
    d("HRANDFIELD", "key [count [WITHVALUES]]", "Returns random fields of a hash"),
    d("HSCAN", "key cursor [MATCH pattern] [COUNT count] [NOVALUES]", "Iterates over the fields of a hash"),
    d("HSET", "key field value [field value ...]", "Sets fields of a hash"),
    d("HSETNX", "key field value", "Sets a hash field only if it doesn't exist"),
    d("HSTRLEN", "key field", "Returns the length of a hash field's value"),
    d("HVALS", "key", "Returns every value of a hash"),
    d("INCR", "key", "Increments the integer value of a key by one"),
    d("INCRBY", "key increment", "Increments the integer value of a key by a number"),
    d("INCRBYFLOAT", "key increment", "Increments the float value of a key by a number"),
    d("INFO", "[section ...]", "Returns information and statistics about the server"),
    d("JSON.GET", "key [path ...]", "Returns JSON values (RedisJSON)"),
    d("JSON.MGET", "key [key ...] path", "Returns a path of several JSON keys (RedisJSON)"),
    d("JSON.SET", "key path value [NX | XX]", "Sets a JSON value (RedisJSON)"),
    d("JSON.TYPE", "key [path]", "Returns the type of a JSON value (RedisJSON)"),
    d("KEYS", "pattern", "Returns every key matching a pattern (blocks a large database: SCAN doesn't)"),
    d("LASTSAVE", "", "Returns the Unix time of the last save to disk"),
    d("LINDEX", "key index", "Returns an element of a list by its index"),
    d("LINSERT", "key BEFORE | AFTER pivot element", "Inserts an element before or after another"),
    d("LLEN", "key", "Returns the length of a list"),
    d("LMOVE", "source destination LEFT | RIGHT LEFT | RIGHT", "Moves an element from one list to another"),
    d("LOLWUT", "[VERSION version]", "Shows the Redis version as computer art"),
    d("LPOP", "key [count]", "Removes and returns the first elements of a list"),
    d("LPOS", "key element [RANK rank] [COUNT num-matches] [MAXLEN len]", "Returns the index of matching elements"),
    d("LPUSH", "key element [element ...]", "Prepends elements to a list"),
    d("LPUSHX", "key element [element ...]", "Prepends elements to a list only if it exists"),
    d("LRANGE", "key start stop", "Returns a range of elements of a list"),
    d("LREM", "key count element", "Removes elements from a list"),
    d("LSET", "key index element", "Sets an element of a list by its index"),
    d("LTRIM", "key start stop", "Keeps only a range of a list"),
    d("MEMORY DOCTOR", "", "Reports memory problems"),
    d("MEMORY STATS", "", "Returns memory usage details"),
    d("MEMORY USAGE", "key [SAMPLES count]", "Returns the memory a key and its value use"),
    d("MGET", "key [key ...]", "Returns the string values of several keys"),
    d("MOVE", "key db", "Moves a key to another database"),
    d("MSET", "key value [key value ...]", "Sets several string keys"),
    d("MSETNX", "key value [key value ...]", "Sets several string keys only if none exists"),
    d("OBJECT ENCODING", "key", "Returns the internal encoding of a value"),
    d("OBJECT FREQ", "key", "Returns the access frequency of a key (LFU)"),
    d("OBJECT IDLETIME", "key", "Returns the seconds since a key was last used"),
    d("OBJECT REFCOUNT", "key", "Returns the reference count of a value"),
    d("PERSIST", "key", "Removes the expiry of a key"),
    d("PEXPIRE", "key milliseconds [NX | XX | GT | LT]", "Sets the expiry of a key in milliseconds"),
    d("PEXPIREAT", "key unix-time-milliseconds [NX | XX | GT | LT]", "Sets the expiry of a key to a Unix time in milliseconds"),
    d("PEXPIRETIME", "key", "Returns the expiry of a key as a Unix time in milliseconds"),
    d("PFADD", "key [element ...]", "Adds elements to a HyperLogLog"),
    d("PFCOUNT", "key [key ...]", "Returns the approximate number of distinct elements"),
    d("PFMERGE", "destkey [sourcekey ...]", "Merges HyperLogLogs"),
    d("PING", "[message]", "Checks the connection"),
    d("PSETEX", "key milliseconds value", "Sets a string with an expiry in milliseconds"),
    d("PTTL", "key", "Returns the expiry of a key in milliseconds"),
    d("PUBLISH", "channel message", "Posts a message to a channel"),
    d("RANDOMKEY", "", "Returns a random key name"),
    d("RENAME", "key newkey", "Renames a key, replacing newkey"),
    d("RENAMENX", "key newkey", "Renames a key only if newkey doesn't exist"),
    d("RESTORE", "key ttl serialized-value [REPLACE] [ABSTTL]", "Creates a key from a DUMP"),
    d("ROLE", "", "Returns the replication role of the server"),
    d("RPOP", "key [count]", "Removes and returns the last elements of a list"),
    d("RPUSH", "key element [element ...]", "Appends elements to a list"),
    d("RPUSHX", "key element [element ...]", "Appends elements to a list only if it exists"),
    d("SADD", "key member [member ...]", "Adds members to a set"),
    d("SCAN", "cursor [MATCH pattern] [COUNT count] [TYPE type]", "Iterates over the key names of the database"),
    d("SCARD", "key", "Returns the number of members in a set"),
    d("SCRIPT FLUSH", "[ASYNC | SYNC]", "Removes every cached Lua script"),
    d("SCRIPT LOAD", "script", "Caches a Lua script"),
    d("SDIFF", "key [key ...]", "Returns the difference of sets"),
    d("SDIFFSTORE", "destination key [key ...]", "Stores the difference of sets in a key"),
    d("SET", "key value [NX | XX] [GET] [EX seconds | PX milliseconds | EXAT unix-time-seconds | PXAT unix-time-milliseconds | KEEPTTL]", "Sets the string value of a key"),
    d("SETBIT", "key offset value", "Sets a bit of a string"),
    d("SETEX", "key seconds value", "Sets a string with an expiry in seconds"),
    d("SETNX", "key value", "Sets a string only if the key doesn't exist"),
    d("SETRANGE", "key offset value", "Overwrites part of a string"),
    d("SINTER", "key [key ...]", "Returns the intersection of sets"),
    d("SINTERCARD", "numkeys key [key ...] [LIMIT limit]", "Returns the size of the intersection of sets"),
    d("SINTERSTORE", "destination key [key ...]", "Stores the intersection of sets in a key"),
    d("SISMEMBER", "key member", "Whether a member is in a set"),
    d("SLOWLOG GET", "[count]", "Returns the slow log's entries"),
    d("SLOWLOG LEN", "", "Returns the number of slow log entries"),
    d("SLOWLOG RESET", "", "Clears the slow log"),
    d("SMEMBERS", "key", "Returns every member of a set"),
    d("SMISMEMBER", "key member [member ...]", "Whether each member is in a set"),
    d("SMOVE", "source destination member", "Moves a member from one set to another"),
    d("SPOP", "key [count]", "Removes and returns random members of a set"),
    d("SRANDMEMBER", "key [count]", "Returns random members of a set"),
    d("SREM", "key member [member ...]", "Removes members from a set"),
    d("SSCAN", "key cursor [MATCH pattern] [COUNT count]", "Iterates over the members of a set"),
    d("STRLEN", "key", "Returns the length of a string"),
    d("SUNION", "key [key ...]", "Returns the union of sets"),
    d("SUNIONSTORE", "destination key [key ...]", "Stores the union of sets in a key"),
    d("SWAPDB", "index1 index2", "Swaps two databases"),
    d("TIME", "", "Returns the server time"),
    d("TOUCH", "key [key ...]", "Updates the last access time of keys"),
    d("TTL", "key", "Returns the expiry of a key in seconds"),
    d("TYPE", "key", "Returns the type of a key's value"),
    d("UNLINK", "key [key ...]", "Deletes keys, freeing their memory in the background"),
    d("XACK", "key group id [id ...]", "Acknowledges stream messages of a consumer group"),
    d("XADD", "key [NOMKSTREAM] [MAXLEN | MINID threshold] * | id field value [field value ...]", "Appends a message to a stream"),
    d("XDEL", "key id [id ...]", "Deletes messages from a stream"),
    d("XINFO", "STREAM | GROUPS | CONSUMERS key", "Returns information about a stream"),
    d("XLEN", "key", "Returns the number of messages in a stream"),
    d("XPENDING", "key group [start end count [consumer]]", "Returns the pending messages of a consumer group"),
    d("XRANGE", "key start end [COUNT count]", "Returns messages of a stream in an id range"),
    d("XREVRANGE", "key end start [COUNT count]", "Returns messages of a stream in an id range, newest first"),
    d("XTRIM", "key MAXLEN | MINID threshold [LIMIT count]", "Removes the oldest messages of a stream"),
    d("ZADD", "key [NX | XX] [GT | LT] [CH] [INCR] score member [score member ...]", "Adds members with scores to a sorted set"),
    d("ZCARD", "key", "Returns the number of members in a sorted set"),
    d("ZCOUNT", "key min max", "Counts the members with scores in a range"),
    d("ZINCRBY", "key increment member", "Increments the score of a member"),
    d("ZLEXCOUNT", "key min max", "Counts the members in a lexicographical range"),
    d("ZMSCORE", "key member [member ...]", "Returns the scores of members"),
    d("ZPOPMAX", "key [count]", "Removes and returns the highest-scored members"),
    d("ZPOPMIN", "key [count]", "Removes and returns the lowest-scored members"),
    d("ZRANDMEMBER", "key [count [WITHSCORES]]", "Returns random members of a sorted set"),
    d("ZRANGE", "key start stop [BYSCORE | BYLEX] [REV] [LIMIT offset count] [WITHSCORES]", "Returns a range of members of a sorted set"),
    d("ZRANGEBYLEX", "key min max [LIMIT offset count]", "Returns members in a lexicographical range"),
    d("ZRANGEBYSCORE", "key min max [WITHSCORES] [LIMIT offset count]", "Returns members with scores in a range"),
    d("ZRANK", "key member [WITHSCORE]", "Returns the rank of a member, lowest score first"),
    d("ZREM", "key member [member ...]", "Removes members from a sorted set"),
    d("ZREMRANGEBYRANK", "key start stop", "Removes members in a rank range"),
    d("ZREMRANGEBYSCORE", "key min max", "Removes members in a score range"),
    d("ZREVRANGE", "key start stop [WITHSCORES]", "Returns a range of members, highest score first"),
    d("ZREVRANGEBYLEX", "key max min [LIMIT offset count]", "Returns members in a lexicographical range, in reverse"),
    d("ZREVRANGEBYSCORE", "key max min [WITHSCORES] [LIMIT offset count]", "Returns members with scores in a range, highest first"),
    d("ZREVRANK", "key member [WITHSCORE]", "Returns the rank of a member, highest score first"),
    d("ZSCAN", "key cursor [MATCH pattern] [COUNT count]", "Iterates over the members of a sorted set"),
    d("ZSCORE", "key member", "Returns the score of a member"),
];

/// The table's entry for a line's words (a container command with its subcommand: "CONFIG GET"), and how many words its name takes
pub fn command_doc(words: &[&str]) -> Option<(&'static CommandDoc, usize)> {
    let first = words.first()?.to_uppercase();
    if let Some(sub) = words.get(1).map(|w| w.to_uppercase())
        && let Some(c) = COMMANDS.iter().find(|c| c.name.split_once(' ') == Some((first.as_str(), sub.as_str())))
    {
        return Some((c, 2));
    }
    COMMANDS.iter().find(|c| c.name == first).map(|c| (c, 1))
}

/// What the Terminal offers for the text before the caret: the commands the word being typed can become, whether the word is
/// a key name (the caller completes those from a bounded SCAN), and the arguments still to type
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Assist {
    /// The typed part of the current word (a pick replaces it)
    pub prefix: String,
    /// Commands (or a container command's subcommands) that start with it
    pub commands: Vec<Pick>,
    /// The word is a key name argument
    pub key: bool,
    /// The arguments from the current one on ("key field"), shown after the caret
    pub hint: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Pick {
    /// What a pick puts in place of the prefix
    pub insert: String,
    pub doc: CommandDoc,
    pub read: bool,
}

pub fn assist(before: &str) -> Assist {
    let words: Vec<&str> = before.split_whitespace().collect();
    let typing = !before.is_empty() && !before.ends_with(char::is_whitespace);
    let prefix = if typing { words.last().copied().unwrap_or("") } else { "" };
    let at = if typing { words.len() - 1 } else { words.len() };
    let pick = |c: &'static CommandDoc, insert: &str| Pick { insert: insert.to_string(), doc: *c, read: c.read() };
    let up = prefix.to_uppercase();
    let mut out = Assist { prefix: prefix.to_string(), commands: vec![], key: false, hint: String::new() };
    if at == 0 {
        out.commands = COMMANDS.iter().filter(|c| !up.is_empty() && c.name.starts_with(&up)).map(|c| pick(c, c.name)).collect();
        return out;
    }
    // The second word of a container command (CONFIG, CLIENT …) is its subcommand
    let head = words[0].to_uppercase();
    if at == 1 && COMMANDS.iter().any(|c| c.name.split_once(' ').is_some_and(|(h, _)| h == head)) {
        out.commands = COMMANDS.iter().filter_map(|c| c.name.split_once(' ').filter(|(h, s)| *h == head && s.starts_with(&up)).map(|(_, s)| pick(c, s))).collect();
        return out;
    }
    let Some((c, n)) = command_doc(&words) else { return out };
    if at < n {
        return out;
    }
    let i = at - n;
    let pos = c.positional();
    out.key = match pos.get(i) {
        Some(t) => *t == "key",
        None => pos.last() == Some(&"key") && c.args.contains("[key ...]"),
    };
    let optional = c.args.find('[').map(|o| &c.args[o..]).unwrap_or("");
    let rest = pos.get(i..).unwrap_or_default().join(" ");
    out.hint = [rest.as_str(), optional].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ");
    out
}

/// The Terminal's HELP: `HELP` alone says how to use the Terminal, `HELP HGET` gives the command's arguments and summary.
/// Answered from the table, without the server. None when the line isn't a HELP
pub fn help(line: &str) -> Option<String> {
    let words: Vec<&str> = line.split_whitespace().collect();
    if !words.first().is_some_and(|w| w.eq_ignore_ascii_case("help")) {
        return None;
    }
    if words.len() == 1 {
        return Some("HELP <command> shows its arguments · ⇥ completes commands and key names · ↑↓ history".into());
    }
    Some(match command_doc(&words[1..]) {
        Some((c, _)) => format!("{}{}{} — {}. ({})", c.name, if c.args.is_empty() { "" } else { " " }, c.args, c.summary, if c.read() { "read" } else { "write" }),
        None => format!("{} isn't in Qurry's command table; a read-only data source treats it as a write", words[1..].join(" ").to_uppercase()),
    })
}

/// How the Terminal shows a command's array reply, beyond redis-cli's numbered lines
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ReplyShape {
    /// As redis-cli prints it
    Plain,
    /// field, value, field, value … (HGETALL, CONFIG GET, HRANDFIELD … WITHVALUES): a field / value table
    Pairs,
    /// member, score … (WITHSCORES, ZPOPMIN / ZPOPMAX): a member / score table
    Scored,
    /// Set members, field or key names: one per row
    Members,
    /// List elements: rows numbered from 0, as LINDEX counts
    List,
}

/// For the line the Terminal ran (read as it ran: parse_command)
pub fn reply_shape(line: &str) -> ReplyShape {
    let args: Vec<String> = parse_command(line).unwrap_or_default().iter().map(|a| String::from_utf8_lossy(a).into_owned()).collect();
    let cmd = args.first().map(|c| c.to_uppercase()).unwrap_or_default();
    let has = |w: &str| args.iter().skip(1).any(|a| a.eq_ignore_ascii_case(w));
    match cmd.as_str() {
        "HGETALL" => ReplyShape::Pairs,
        "CONFIG" if args.get(1).is_some_and(|s| s.eq_ignore_ascii_case("GET")) => ReplyShape::Pairs,
        "HRANDFIELD" if has("WITHVALUES") => ReplyShape::Pairs,
        "ZPOPMIN" | "ZPOPMAX" => ReplyShape::Scored,
        "ZRANGE" | "ZREVRANGE" | "ZRANGEBYSCORE" | "ZREVRANGEBYSCORE" | "ZRANDMEMBER" if has("WITHSCORES") => ReplyShape::Scored,
        "SMEMBERS" | "SINTER" | "SUNION" | "SDIFF" | "SRANDMEMBER" | "SPOP" | "HKEYS" | "HVALS" | "KEYS" | "ZRANGE" | "ZREVRANGE" | "ZRANGEBYSCORE" | "ZREVRANGEBYSCORE"
        | "ZRANGEBYLEX" | "ZREVRANGEBYLEX" | "ZRANDMEMBER" => ReplyShape::Members,
        "LRANGE" => ReplyShape::List,
        _ => ReplyShape::Plain,
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

    /// The Terminal's table and the guard agree: a row's read flag is the guard's answer, every command the guard reads is in the
    /// table, the ones that change or delete data are writes, and anything unknown is a write (a read-only source refuses it)
    #[test]
    fn command_table_agrees_with_the_guard() {
        let names: Vec<&str> = COMMANDS.iter().map(|c| c.name).collect();
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(names, sorted, "sorted, no duplicates");
        for c in COMMANDS {
            let mut w = c.name.split(' ');
            assert_eq!(c.read(), is_read_command(w.next().unwrap(), w.next().unwrap_or("")), "{}", c.name);
        }
        for r in READ.iter().filter(|r| **r != "DBTOOL.VIEW") {
            assert!(command_doc(&[r]).is_some_and(|(c, _)| c.read()), "{r} reads but isn't a read row of the table");
        }
        let class = |line: &str| command_doc(&line.split(' ').collect::<Vec<_>>()).map(|(c, _)| c.read());
        for w in ["SET", "DEL", "UNLINK", "FLUSHDB", "FLUSHALL", "EVAL", "CONFIG SET", "CONFIG REWRITE", "RENAME", "EXPIRE", "HSET", "SWAPDB", "MOVE", "CLIENT KILL"] {
            assert_eq!(class(w), Some(false), "{w} is a write");
        }
        for r in ["GET", "HGETALL", "CONFIG GET", "MEMORY USAGE", "SCAN", "TTL"] {
            assert_eq!(class(r), Some(true), "{r} is a read");
        }
        assert!(command_doc(&["NOSUCH"]).is_none() && !is_read_command("NOSUCH", ""));
    }

    #[test]
    fn assist_completes_commands_keys_and_hints() {
        let a = assist("HG");
        assert_eq!(a.prefix, "HG");
        assert_eq!(a.commands.iter().map(|p| p.insert.as_str()).collect::<Vec<_>>(), ["HGET", "HGETALL"]);
        assert_eq!((a.commands[0].doc.args, a.commands[0].read), ("key field", true));
        assert!(assist("").commands.is_empty(), "nothing typed: nothing listed");
        // A key argument (completed from SCAN by the caller) and the arguments still to type
        let a = assist("hget prod:u");
        assert!(a.key && a.prefix == "prod:u" && a.hint == "key field" && a.commands.is_empty());
        let a = assist("HGET prod:user:1 ");
        assert!(!a.key && a.prefix.is_empty() && a.hint == "field");
        assert!(assist("DEL a b ").key, "DEL key [key ...]: every argument is a key");
        assert!(!assist("SET k ").key);
        assert_eq!(assist("SET k v ").hint, "[NX | XX] [GET] [EX seconds | PX milliseconds | EXAT unix-time-seconds | PXAT unix-time-milliseconds | KEEPTTL]");
        // Container commands: the second word is the subcommand
        assert_eq!(assist("CONFIG ").commands.iter().map(|p| p.insert.as_str()).collect::<Vec<_>>(), ["GET", "RESETSTAT", "REWRITE", "SET"]);
        let a = assist("config s");
        assert_eq!((a.commands.len(), a.commands[0].insert.as_str(), a.commands[0].read), (1, "SET", false));
        assert_eq!(assist("CONFIG GET ").hint, "parameter [parameter ...]");
        assert_eq!(assist("NOSUCH x").hint, "");
    }

    #[test]
    fn help_answers_from_the_table() {
        assert_eq!(help("HELP hget").unwrap(), "HGET key field — Returns the value of a field in a hash. (read)");
        assert_eq!(help("help config set").unwrap(), "CONFIG SET parameter value [parameter value ...] — Sets configuration parameters. (write)");
        assert_eq!(help("help dbsize").unwrap(), "DBSIZE — Returns the number of keys in the database. (read)");
        assert!(help("HELP nosuch").unwrap().contains("treats it as a write"));
        assert!(help("HELP").unwrap().contains("HELP <command>"));
        assert!(help("HGET k f").is_none() && help("HELLO 3").is_none());
    }

    #[test]
    fn reply_shapes() {
        let s = reply_shape;
        assert_eq!(s("\"HGETALL\" h"), ReplyShape::Pairs);
        assert_eq!(s("hgetall h"), ReplyShape::Pairs);
        assert_eq!(s("CONFIG GET maxmemory*"), ReplyShape::Pairs);
        assert_eq!(s("ZRANGE z 0 -1 withscores"), ReplyShape::Scored);
        assert_eq!(s("ZRANGE z 0 -1"), ReplyShape::Members);
        assert_eq!(s("SMEMBERS s"), ReplyShape::Members);
        assert_eq!(s("LRANGE l 0 -1"), ReplyShape::List);
        assert_eq!(s("MGET a b"), ReplyShape::Plain);
    }
}
