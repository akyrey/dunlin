/** Redis commands in the editor (D-032): the command word highlighted and completed, quoted
 * arguments as strings, `#` and `//` lines as comments. One command per line. */
import { type Completion, type CompletionContext, autocompletion } from "@codemirror/autocomplete";
import { StreamLanguage } from "@codemirror/language";
import type { Extension } from "@codemirror/state";

/** Commands offered while typing; any other command still runs. */
export const REDIS_COMMANDS = [
  "APPEND",
  "BITCOUNT",
  "BLPOP",
  "BRPOP",
  "COPY",
  "DBSIZE",
  "DECR",
  "DECRBY",
  "DEL",
  "DUMP",
  "ECHO",
  "EXISTS",
  "EXPIRE",
  "EXPIREAT",
  "EXPIRETIME",
  "GET",
  "GETDEL",
  "GETEX",
  "GETRANGE",
  "GETSET",
  "HDEL",
  "HEXISTS",
  "HGET",
  "HGETALL",
  "HINCRBY",
  "HINCRBYFLOAT",
  "HKEYS",
  "HLEN",
  "HMGET",
  "HRANDFIELD",
  "HSCAN",
  "HSET",
  "HSETNX",
  "HSTRLEN",
  "HVALS",
  "INCR",
  "INCRBY",
  "INCRBYFLOAT",
  "INFO",
  "KEYS",
  "LINDEX",
  "LINSERT",
  "LLEN",
  "LMOVE",
  "LPOP",
  "LPOS",
  "LPUSH",
  "LPUSHX",
  "LRANGE",
  "LREM",
  "LSET",
  "LTRIM",
  "MEMORY",
  "MGET",
  "MSET",
  "MSETNX",
  "OBJECT",
  "PERSIST",
  "PEXPIRE",
  "PFADD",
  "PFCOUNT",
  "PING",
  "PTTL",
  "PUBLISH",
  "RANDOMKEY",
  "RENAME",
  "RENAMENX",
  "RPOP",
  "RPUSH",
  "RPUSHX",
  "SADD",
  "SCAN",
  "SCARD",
  "SDIFF",
  "SELECT",
  "SET",
  "SETEX",
  "SETNX",
  "SETRANGE",
  "SINTER",
  "SISMEMBER",
  "SMEMBERS",
  "SMOVE",
  "SPOP",
  "SRANDMEMBER",
  "SREM",
  "SSCAN",
  "STRLEN",
  "SUNION",
  "TIME",
  "TOUCH",
  "TTL",
  "TYPE",
  "UNLINK",
  "XADD",
  "XDEL",
  "XINFO",
  "XLEN",
  "XRANGE",
  "XREAD",
  "XREVRANGE",
  "XTRIM",
  "ZADD",
  "ZCARD",
  "ZCOUNT",
  "ZINCRBY",
  "ZRANGE",
  "ZRANGEBYSCORE",
  "ZRANK",
  "ZREM",
  "ZREVRANGE",
  "ZREVRANK",
  "ZSCAN",
  "ZSCORE",
  "CLIENT",
  "CONFIG",
  "COMMAND",
  "MULTI",
  "EXEC",
  "DISCARD",
  "WATCH",
  "EVAL",
  "EVALSHA",
  "SCRIPT",
  "FLUSHDB",
  "FLUSHALL",
];

type State = { first: boolean };

/** The tokenizer; exported for its tests. */
export const redis = StreamLanguage.define<State>({
  name: "redis",
  startState: () => ({ first: true }),
  token(stream, state) {
    if (stream.sol()) state.first = true;
    if (stream.eatSpace()) return null;
    if (state.first && (stream.match("#") || stream.match("//"))) {
      stream.skipToEnd();
      return "comment";
    }
    const quote = stream.peek();
    if (quote === '"' || quote === "'") {
      stream.next();
      let escaped = false;
      for (let c = stream.next(); c !== undefined; c = stream.next()) {
        if (c === quote && !escaped) break;
        escaped = !escaped && c === "\\";
      }
      state.first = false;
      return "string";
    }
    stream.eatWhile(/\S/);
    const word = stream.current();
    const wasFirst = state.first;
    state.first = false;
    if (wasFirst) return "keyword";
    return /^-?\d+(\.\d+)?$/.test(word) ? "number" : null;
  },
  languageData: { commentTokens: { line: "#" } },
});

const OPTIONS: Completion[] = REDIS_COMMANDS.map((label) => ({ label, type: "keyword" }));

/** Completes the command word at the start of a line. */
export function completeCommand(context: CompletionContext) {
  const line = context.state.doc.lineAt(context.pos);
  const before = line.text.slice(0, context.pos - line.from);
  const match = /^\s*(\S*)$/.exec(before);
  if (!match) return null;
  const typed = match[1] ?? "";
  if (!typed && !context.explicit) return null;
  return { from: context.pos - typed.length, options: OPTIONS, validFor: /^\S*$/ };
}

export function redisLanguage(): Extension {
  return [redis, autocompletion({ override: [completeCommand] })];
}
