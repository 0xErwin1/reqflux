//! The JavaScript prelude that builds `pm`, `pm.expect` and `console` over the host functions.

/// The shim installed before every script: it defines `pm` and `console` in terms of the
/// `__reqflux*` host functions and the `__reqfluxInit` context JSON the sandbox evaluates first.
pub(super) const PRELUDE: &str = r#"
"use strict";
(function () {
  const init = globalThis.__reqfluxInit;
  delete globalThis.__reqfluxInit;

  function stringify(v) {
    if (typeof v === "string") return v;
    if (v === undefined) return "undefined";
    if (v === null) return "null";
    if (typeof v === "number" || typeof v === "boolean") return String(v);
    if (typeof v === "function") return "[Function" + (v.name ? ": " + v.name : "") + "]";
    if (typeof v === "symbol" || typeof v === "bigint") return String(v);
    try { return JSON.stringify(v); } catch (e) { return "[Unserializable]"; }
  }

  function deepEq(a, b) {
    if (a === b) return true;
    if (a === null || b === null || typeof a !== "object" || typeof b !== "object") {
      return typeof a === "number" && typeof b === "number" && Number.isNaN(a) && Number.isNaN(b);
    }
    if (Array.isArray(a) !== Array.isArray(b)) return false;
    const ka = Object.keys(a);
    const kb = Object.keys(b);
    if (ka.length !== kb.length) return false;
    return ka.every((k) => Object.prototype.hasOwnProperty.call(b, k) && deepEq(a[k], b[k]));
  }

  function makeScope(name) {
    return {
      get(key) { return __reqfluxVarGet(name, String(key)); },
      set(key, value) { __reqfluxVarSet(name, String(key), stringify(value)); },
      unset(key) { __reqfluxVarUnset(name, String(key)); },
    };
  }

  function makeExpect(value, customMessage) {
    const prefix = customMessage !== undefined ? String(customMessage) + ": " : "";
    function fail(text) { throw new Error(prefix + text); }
    function assert(ok, text) { if (!ok) fail(text); }
    function show(v) { return stringify(v); }
    function checkType(type) {
      const actual = value === null ? "null" : Array.isArray(value) ? "array" : typeof value;
      assert(actual === type, "expected " + show(value) + " to be a " + type + " (got " + actual + ")");
      return makeAssertion();
    }
    function checkOk() {
      const blank = value === null || value === undefined || value === false || value === ""
        || (typeof value === "number" && (Number.isNaN(value) || value === 0));
      assert(!blank, "expected " + show(value) + " to be ok");
      return makeAssertion();
    }
    function checkFlag(expected) {
      assert(value === expected, "expected " + show(value) + " to be " + String(expected));
      return makeAssertion();
    }
    function checkNull() {
      assert(value === null, "expected " + show(value) + " to be null");
      return makeAssertion();
    }
    function checkEqual(expected) {
      assert(deepEq(value, expected), "expected " + show(value) + " to deeply equal " + show(expected));
      return makeAssertion();
    }
    function checkProperty(name, expected) {
      const has = value !== null && value !== undefined
        && Object.prototype.hasOwnProperty.call(Object(value), name);
      assert(has, "expected " + show(value) + " to have a property " + show(name));
      if (arguments.length > 1) {
        assert(deepEq(value[name], expected), "expected property " + show(name) + " to equal " + show(expected));
      }
      return makeAssertion();
    }
    function checkInclude(expected) {
      if (typeof value === "string") {
        assert(value.includes(String(expected)), "expected " + show(value) + " to include " + show(expected));
      } else if (Array.isArray(value)) {
        assert(value.some((item) => deepEq(item, expected)), "expected " + show(value) + " to include " + show(expected));
      } else if (value !== null && typeof value === "object") {
        const keys = Object.keys(expected);
        const subset = keys.every((k) => Object.prototype.hasOwnProperty.call(value, k) && deepEq(value[k], expected[k]));
        assert(subset, "expected " + show(value) + " to include " + show(expected));
      } else {
        fail("cannot use include on " + show(value));
      }
      return makeAssertion();
    }
    function checkAbove(expected) {
      assert(typeof value === "number" && value > expected, "expected " + show(value) + " to be above " + show(expected));
      return makeAssertion();
    }
    function checkBelow(expected) {
      assert(typeof value === "number" && value < expected, "expected " + show(value) + " to be below " + show(expected));
      return makeAssertion();
    }
    function checkMatch(pattern) {
      assert(new RegExp(pattern).test(String(value)), "expected " + show(value) + " to match " + show(pattern));
      return makeAssertion();
    }
    function okAccessor() {
      checkOk();
      return () => checkOk();
    }
    function flagAccessor(expected) {
      // Asserting on access covers `to.be.true` used as a property; returning a callable that
      // re-asserts covers `to.be.true()` used as a call.
      checkFlag(expected);
      return () => checkFlag(expected);
    }
    function nullAccessor() {
      checkNull();
      return () => checkNull();
    }
    // Chai lets collections chain anywhere (to.be.above, to.have.property), so every link
    // resolves the same table of matchers and never passes an unknown one silently.
    function makeChain() {
      return new Proxy({}, {
        get(_target, prop) {
          switch (String(prop)) {
            case "equal": return checkEqual;
            case "eql": return checkEqual;
            case "include": return checkInclude;
            case "above": return checkAbove;
            case "below": return checkBelow;
            case "match": return checkMatch;
            case "property": return checkProperty;
            case "a":
            case "an": return checkType;
            case "ok": return okAccessor();
            case "true": return flagAccessor(true);
            case "false": return flagAccessor(false);
            case "null": return nullAccessor();
            case "be":
            case "been":
            case "is":
            case "and":
            case "has":
            case "have":
            case "with":
            case "that":
            case "which":
            case "to": return makeChain();
            default: throw new Error("unknown matcher: to." + String(prop));
          }
        },
      });
    }
    function makeAssertion() {
      const assertion = {};
      assertion.to = makeChain();
      return assertion;
    }
    return makeAssertion();
  }

  function makeHeaders(map) {
    return {
      get(name) {
        const lower = String(name).toLowerCase();
        const keys = Object.keys(map);
        for (let i = 0; i < keys.length; i++) {
          if (keys[i].toLowerCase() === lower) return map[keys[i]];
        }
        return undefined;
      },
    };
  }

  // `pm.response.to`: Postman's chai-style assertions on the response. `not` flips the
  // verdict for everything after it; a failure carries the positive or the negated message.
  function makeResponseChain(response, rawHeaders, negate) {
    function verify(pass, positive, negative) {
      if (pass === negate) throw new Error(negate ? negative : positive);
      return makeResponseChain(response, rawHeaders, negate);
    }
    function parsedBody() {
      try { return { ok: true, value: response.json() }; } catch (e) { return { ok: false }; }
    }
    function headerValue(name) {
      const lower = String(name).toLowerCase();
      const key = Object.keys(rawHeaders).find((k) => k.toLowerCase() === lower);
      return key === undefined ? undefined : rawHeaders[key];
    }
    function status(expected) {
      if (typeof expected === "number") {
        return verify(
          response.code === expected,
          "expected response to have status code " + expected + " but got " + response.code,
          "expected response to not have status code " + expected);
      }
      return verify(
        response.status === String(expected),
        "expected response to have status reason '" + expected + "' but got '" + response.status + "'",
        "expected response to not have status reason '" + expected + "'");
    }
    function header(name, value) {
      const actual = headerValue(name);
      if (arguments.length < 2) {
        return verify(
          actual !== undefined,
          "expected response to have header with key '" + name + "'",
          "expected response to not have header with key '" + name + "'");
      }
      const base = "header with key '" + name + "' and value '" + value + "'";
      return verify(
        actual === String(value),
        "expected response to have " + base + (actual === undefined ? "" : ", got '" + actual + "'"),
        "expected response to not have " + base);
    }
    function body(expected) {
      const text = response._text;
      if (expected instanceof RegExp) {
        return verify(
          expected.test(text),
          "expected response body to match " + String(expected) + " but got '" + text + "'",
          "expected response body to not match " + String(expected));
      }
      return verify(
        text === String(expected),
        "expected response body to equal '" + expected + "' but got '" + text + "'",
        "expected response body to not equal '" + expected + "'");
    }
    function jsonBody(path, value) {
      const parsed = parsedBody();
      if (!parsed.ok) {
        return verify(false, "expected response body to be valid JSON", "expected response body to not be valid JSON");
      }
      if (arguments.length === 0) {
        return verify(true, "", "expected response body to not be valid JSON");
      }
      if (typeof path !== "string") {
        return verify(
          deepEq(parsed.value, path),
          "expected response json body to equal " + stringify(path) + " but got " + stringify(parsed.value),
          "expected response json body to not equal " + stringify(path));
      }
      let found = parsed.value;
      for (const part of path.split(".")) {
        if (found === null || found === undefined || !Object.hasOwn(Object(found), part)) {
          found = undefined;
          break;
        }
        found = found[part];
      }
      if (arguments.length < 2) {
        return verify(
          found !== undefined,
          "expected response json body to have '" + path + "'",
          "expected response json body to not have '" + path + "'");
      }
      return verify(
        deepEq(found, value),
        "expected response json body at '" + path + "' to equal " + stringify(value) + " but got " + stringify(found),
        "expected response json body at '" + path + "' to not equal " + stringify(value));
    }
    const flags = {
      ok: () => verify(
        response.code === 200,
        "expected response to be ok (status 200) but got " + response.code,
        "expected response to not be ok (status 200)"),
      success: () => verify(
        response.code >= 200 && response.code < 300,
        "expected response to be successful (status 2xx) but got " + response.code,
        "expected response to not be successful (status 2xx)"),
      error: () => verify(
        response.code >= 400 && response.code < 600,
        "expected response to be an error (status 4xx or 5xx) but got " + response.code,
        "expected response to not be an error (status 4xx or 5xx)"),
      clientError: () => verify(
        response.code >= 400 && response.code < 500,
        "expected response to be a client error (status 4xx) but got " + response.code,
        "expected response to not be a client error (status 4xx)"),
      serverError: () => verify(
        response.code >= 500 && response.code < 600,
        "expected response to be a server error (status 5xx) but got " + response.code,
        "expected response to not be a server error (status 5xx)"),
      json: () => verify(
        parsedBody().ok,
        "expected response body to be valid JSON",
        "expected response body to not be valid JSON"),
    };
    return new Proxy({}, {
      get(_target, prop) {
        const name = String(prop);
        switch (name) {
          case "status": return status;
          case "header": return header;
          case "body": return body;
          case "jsonBody": return jsonBody;
          case "not": return makeResponseChain(response, rawHeaders, !negate);
          case "to":
          case "be":
          case "been":
          case "is":
          case "have":
          case "has":
          case "and":
          case "with":
          case "that":
          case "which": return makeResponseChain(response, rawHeaders, negate);
          default:
            if (Object.hasOwn(flags, name)) return flags[name]();
            throw new Error("unknown matcher: to." + name);
        }
      },
    });
  }

  function makeSendResponse(data) {
    const response = { code: data.code, status: data.status, headers: makeHeaders(data.headers) };
    response.text = function () { return data.text; };
    let cached;
    response.json = function () {
      if (cached === undefined) cached = JSON.parse(data.text);
      return cached;
    };
    return response;
  }

  function sendHeaders(header) {
    if (header === undefined || header === null) return [];
    if (Array.isArray(header)) {
      return header.map(function (row) { return [String(row.key), String(row.value)]; });
    }
    return Object.keys(header).map(function (key) { return [key, String(header[key])]; });
  }

  function sendBody(body) {
    if (body === undefined || body === null) return null;
    if (typeof body === "string") return body;
    if (typeof body === "object" && typeof body.raw === "string") return body.raw;
    return String(body);
  }

  function sendSpec(arg) {
    if (typeof arg === "string") return { method: "GET", url: arg, headers: [], body: null };
    if (arg === null || typeof arg !== "object") {
      throw new Error("pm.sendRequest expects a URL string or an object");
    }
    if (typeof arg.url !== "string" || arg.url === "") {
      throw new Error("pm.sendRequest expects a url on the request object");
    }
    return {
      method: typeof arg.method === "string" && arg.method !== "" ? arg.method : "GET",
      url: arg.url,
      headers: sendHeaders(arg.header),
      body: sendBody(arg.body),
    };
  }

  // A collection run reads the last call: a name or id jumps there, null stops the run.
  function setNextRequest(target) {
    if (typeof target !== "string" && target !== null) {
      throw new TypeError("setNextRequest expects a request name, an id or null");
    }
    __reqfluxNext(target);
  }

  const pm = {
    variables: makeScope("variables"),
    environment: makeScope("environment"),
    collectionVariables: makeScope("collectionVariables"),
    globals: makeScope("globals"),
    info: { eventName: init.stage === "test" ? "test" : "prerequest" },
    request: init.request,
    test(name, fn) {
      try {
        if (typeof fn !== "function") throw new Error("pm.test expects a function as its second argument");
        fn();
        __reqfluxTest(String(name), true, undefined);
      } catch (e) {
        __reqfluxTest(String(name), false, e && e.message !== undefined ? String(e.message) : String(e));
      }
    },
    expect: makeExpect,
    setNextRequest,
    execution: { setNextRequest },
    sendRequest(arg, callback) {
      if (typeof callback !== "function") {
        throw new Error("pm.sendRequest expects a function as its second argument");
      }
      let spec;
      try {
        spec = sendSpec(arg);
      } catch (e) {
        callback(String(e && e.message !== undefined ? e.message : e), undefined);
        return;
      }
      // __reqfluxSend answers with a JSON document: {"ok":true,"response":...} or
      // {"ok":false,"error":...}. A limit abort throws out of the host call and ends the
      // run — it is never the callback's error.
      const answer = JSON.parse(__reqfluxSend(JSON.stringify(spec)));
      // Postman hands the callback `null` for the side that did not happen, and collections
      // rely on it (`pm.expect(err).to.be.null`), so neither slot is left undefined.
      if (answer.ok) {
        callback(null, makeSendResponse(answer.response));
      } else {
        callback(String(answer.error), null);
      }
    },
  };
  if (pm.request) {
    pm.request.headers = makeHeaders(init.request.headers);
    if (typeof pm.request.body === "string") {
      pm.request.body = { raw: pm.request.body };
    } else if (pm.request.body === null) {
      pm.request.body = undefined;
    }
  }
  if (init.response != null) {
    const response = init.response;
    const rawHeaders = response.headers;
    response.headers = makeHeaders(rawHeaders);
    response.text = function () { return response._text; };
    let cached;
    response.json = function () {
      if (cached === undefined) {
        cached = Object.hasOwn(response, "_parsed") ? response._parsed : JSON.parse(response._text);
      }
      return cached;
    };
    response.to = makeResponseChain(response, rawHeaders, false);
    pm.response = response;
  }
  globalThis.pm = pm;
  globalThis.postman = { setNextRequest };
  globalThis.console = {
    log(...args) { __reqfluxConsole("log", args.map(stringify).join(" ")); },
    info(...args) { __reqfluxConsole("info", args.map(stringify).join(" ")); },
    warn(...args) { __reqfluxConsole("warn", args.map(stringify).join(" ")); },
    error(...args) { __reqfluxConsole("error", args.map(stringify).join(" ")); },
  };
})();
// `require` serves the libraries Postman's sandbox ships. A module is compiled the first time it
// is asked for and cached for the rest of the run, like Node does.
const __modules = Object.create(null);
function require(name) {
  const key = String(name);
  if (__modules[key]) return __modules[key].exports;
  const source = __reqfluxModule(key);
  if (source === null || source === undefined) {
    throw new Error("Cannot find module '" + key + "'");
  }
  const module = { exports: {} };
  __modules[key] = module;
  const factory = new Function("module", "exports", "require", source);
  factory(module, module.exports, require);
  return module.exports;
}
globalThis.require = require;
"#;
