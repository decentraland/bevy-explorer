'use strict';

var EngineApi = require('~system/EngineApi');

var commonjsGlobal = typeof globalThis !== 'undefined' ? globalThis : typeof undefined !== 'undefined' ? undefined : typeof global !== 'undefined' ? global : typeof self !== 'undefined' ? self : {};

function getDefaultExportFromCjs (x) {
	return x && x.__esModule && Object.prototype.hasOwnProperty.call(x, 'default') ? x['default'] : x;
}

var minimalExports$1 = {};
var minimal$1 = {
  get exports(){ return minimalExports$1; },
  set exports(v){ minimalExports$1 = v; },
};

var indexMinimal = {};

var minimal = {};

var aspromise;
var hasRequiredAspromise;

function requireAspromise () {
	if (hasRequiredAspromise) return aspromise;
	hasRequiredAspromise = 1;
	aspromise = asPromise;

	/**
	 * Callback as used by {@link util.asPromise}.
	 * @typedef asPromiseCallback
	 * @type {function}
	 * @param {Error|null} error Error, if any
	 * @param {...*} params Additional arguments
	 * @returns {undefined}
	 */

	/**
	 * Returns a promise from a node-style callback function.
	 * @memberof util
	 * @param {asPromiseCallback} fn Function to call
	 * @param {*} ctx Function context
	 * @param {...*} params Function arguments
	 * @returns {Promise<*>} Promisified function
	 */
	function asPromise(fn, ctx/*, varargs */) {
	    var params  = new Array(arguments.length - 1),
	        offset  = 0,
	        index   = 2,
	        pending = true;
	    while (index < arguments.length)
	        params[offset++] = arguments[index++];
	    return new Promise(function executor(resolve, reject) {
	        params[offset] = function callback(err/*, varargs */) {
	            if (pending) {
	                pending = false;
	                if (err)
	                    reject(err);
	                else {
	                    var params = new Array(arguments.length - 1),
	                        offset = 0;
	                    while (offset < params.length)
	                        params[offset++] = arguments[offset];
	                    resolve.apply(null, params);
	                }
	            }
	        };
	        try {
	            fn.apply(ctx || null, params);
	        } catch (err) {
	            if (pending) {
	                pending = false;
	                reject(err);
	            }
	        }
	    });
	}
	return aspromise;
}

var base64 = {};

var hasRequiredBase64;

function requireBase64 () {
	if (hasRequiredBase64) return base64;
	hasRequiredBase64 = 1;
	(function (exports) {

		/**
		 * A minimal base64 implementation for number arrays.
		 * @memberof util
		 * @namespace
		 */
		var base64 = exports;

		/**
		 * Calculates the byte length of a base64 encoded string.
		 * @param {string} string Base64 encoded string
		 * @returns {number} Byte length
		 */
		base64.length = function length(string) {
		    var p = string.length;
		    if (!p)
		        return 0;
		    var n = 0;
		    while (--p % 4 > 1 && string.charAt(p) === "=")
		        ++n;
		    return Math.ceil(string.length * 3) / 4 - n;
		};

		// Base64 encoding table
		var b64 = new Array(64);

		// Base64 decoding table
		var s64 = new Array(123);

		// 65..90, 97..122, 48..57, 43, 47
		for (var i = 0; i < 64;)
		    s64[b64[i] = i < 26 ? i + 65 : i < 52 ? i + 71 : i < 62 ? i - 4 : i - 59 | 43] = i++;

		/**
		 * Encodes a buffer to a base64 encoded string.
		 * @param {Uint8Array} buffer Source buffer
		 * @param {number} start Source start
		 * @param {number} end Source end
		 * @returns {string} Base64 encoded string
		 */
		base64.encode = function encode(buffer, start, end) {
		    var parts = null,
		        chunk = [];
		    var i = 0, // output index
		        j = 0, // goto index
		        t;     // temporary
		    while (start < end) {
		        var b = buffer[start++];
		        switch (j) {
		            case 0:
		                chunk[i++] = b64[b >> 2];
		                t = (b & 3) << 4;
		                j = 1;
		                break;
		            case 1:
		                chunk[i++] = b64[t | b >> 4];
		                t = (b & 15) << 2;
		                j = 2;
		                break;
		            case 2:
		                chunk[i++] = b64[t | b >> 6];
		                chunk[i++] = b64[b & 63];
		                j = 0;
		                break;
		        }
		        if (i > 8191) {
		            (parts || (parts = [])).push(String.fromCharCode.apply(String, chunk));
		            i = 0;
		        }
		    }
		    if (j) {
		        chunk[i++] = b64[t];
		        chunk[i++] = 61;
		        if (j === 1)
		            chunk[i++] = 61;
		    }
		    if (parts) {
		        if (i)
		            parts.push(String.fromCharCode.apply(String, chunk.slice(0, i)));
		        return parts.join("");
		    }
		    return String.fromCharCode.apply(String, chunk.slice(0, i));
		};

		var invalidEncoding = "invalid encoding";

		/**
		 * Decodes a base64 encoded string to a buffer.
		 * @param {string} string Source string
		 * @param {Uint8Array} buffer Destination buffer
		 * @param {number} offset Destination offset
		 * @returns {number} Number of bytes written
		 * @throws {Error} If encoding is invalid
		 */
		base64.decode = function decode(string, buffer, offset) {
		    var start = offset;
		    var j = 0, // goto index
		        t;     // temporary
		    for (var i = 0; i < string.length;) {
		        var c = string.charCodeAt(i++);
		        if (c === 61 && j > 1)
		            break;
		        if ((c = s64[c]) === undefined)
		            throw Error(invalidEncoding);
		        switch (j) {
		            case 0:
		                t = c;
		                j = 1;
		                break;
		            case 1:
		                buffer[offset++] = t << 2 | (c & 48) >> 4;
		                t = c;
		                j = 2;
		                break;
		            case 2:
		                buffer[offset++] = (t & 15) << 4 | (c & 60) >> 2;
		                t = c;
		                j = 3;
		                break;
		            case 3:
		                buffer[offset++] = (t & 3) << 6 | c;
		                j = 0;
		                break;
		        }
		    }
		    if (j === 1)
		        throw Error(invalidEncoding);
		    return offset - start;
		};

		/**
		 * Tests if the specified string appears to be base64 encoded.
		 * @param {string} string String to test
		 * @returns {boolean} `true` if probably base64 encoded, otherwise false
		 */
		base64.test = function test(string) {
		    return /^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(string);
		};
} (base64));
	return base64;
}

var eventemitter;
var hasRequiredEventemitter;

function requireEventemitter () {
	if (hasRequiredEventemitter) return eventemitter;
	hasRequiredEventemitter = 1;
	eventemitter = EventEmitter;

	/**
	 * Constructs a new event emitter instance.
	 * @classdesc A minimal event emitter.
	 * @memberof util
	 * @constructor
	 */
	function EventEmitter() {

	    /**
	     * Registered listeners.
	     * @type {Object.<string,*>}
	     * @private
	     */
	    this._listeners = {};
	}

	/**
	 * Registers an event listener.
	 * @param {string} evt Event name
	 * @param {function} fn Listener
	 * @param {*} [ctx] Listener context
	 * @returns {util.EventEmitter} `this`
	 */
	EventEmitter.prototype.on = function on(evt, fn, ctx) {
	    (this._listeners[evt] || (this._listeners[evt] = [])).push({
	        fn  : fn,
	        ctx : ctx || this
	    });
	    return this;
	};

	/**
	 * Removes an event listener or any matching listeners if arguments are omitted.
	 * @param {string} [evt] Event name. Removes all listeners if omitted.
	 * @param {function} [fn] Listener to remove. Removes all listeners of `evt` if omitted.
	 * @returns {util.EventEmitter} `this`
	 */
	EventEmitter.prototype.off = function off(evt, fn) {
	    if (evt === undefined)
	        this._listeners = {};
	    else {
	        if (fn === undefined)
	            this._listeners[evt] = [];
	        else {
	            var listeners = this._listeners[evt];
	            for (var i = 0; i < listeners.length;)
	                if (listeners[i].fn === fn)
	                    listeners.splice(i, 1);
	                else
	                    ++i;
	        }
	    }
	    return this;
	};

	/**
	 * Emits an event by calling its listeners with the specified arguments.
	 * @param {string} evt Event name
	 * @param {...*} args Arguments
	 * @returns {util.EventEmitter} `this`
	 */
	EventEmitter.prototype.emit = function emit(evt) {
	    var listeners = this._listeners[evt];
	    if (listeners) {
	        var args = [],
	            i = 1;
	        for (; i < arguments.length;)
	            args.push(arguments[i++]);
	        for (i = 0; i < listeners.length;)
	            listeners[i].fn.apply(listeners[i++].ctx, args);
	    }
	    return this;
	};
	return eventemitter;
}

var float;
var hasRequiredFloat;

function requireFloat () {
	if (hasRequiredFloat) return float;
	hasRequiredFloat = 1;

	float = factory(factory);

	/**
	 * Reads / writes floats / doubles from / to buffers.
	 * @name util.float
	 * @namespace
	 */

	/**
	 * Writes a 32 bit float to a buffer using little endian byte order.
	 * @name util.float.writeFloatLE
	 * @function
	 * @param {number} val Value to write
	 * @param {Uint8Array} buf Target buffer
	 * @param {number} pos Target buffer offset
	 * @returns {undefined}
	 */

	/**
	 * Writes a 32 bit float to a buffer using big endian byte order.
	 * @name util.float.writeFloatBE
	 * @function
	 * @param {number} val Value to write
	 * @param {Uint8Array} buf Target buffer
	 * @param {number} pos Target buffer offset
	 * @returns {undefined}
	 */

	/**
	 * Reads a 32 bit float from a buffer using little endian byte order.
	 * @name util.float.readFloatLE
	 * @function
	 * @param {Uint8Array} buf Source buffer
	 * @param {number} pos Source buffer offset
	 * @returns {number} Value read
	 */

	/**
	 * Reads a 32 bit float from a buffer using big endian byte order.
	 * @name util.float.readFloatBE
	 * @function
	 * @param {Uint8Array} buf Source buffer
	 * @param {number} pos Source buffer offset
	 * @returns {number} Value read
	 */

	/**
	 * Writes a 64 bit double to a buffer using little endian byte order.
	 * @name util.float.writeDoubleLE
	 * @function
	 * @param {number} val Value to write
	 * @param {Uint8Array} buf Target buffer
	 * @param {number} pos Target buffer offset
	 * @returns {undefined}
	 */

	/**
	 * Writes a 64 bit double to a buffer using big endian byte order.
	 * @name util.float.writeDoubleBE
	 * @function
	 * @param {number} val Value to write
	 * @param {Uint8Array} buf Target buffer
	 * @param {number} pos Target buffer offset
	 * @returns {undefined}
	 */

	/**
	 * Reads a 64 bit double from a buffer using little endian byte order.
	 * @name util.float.readDoubleLE
	 * @function
	 * @param {Uint8Array} buf Source buffer
	 * @param {number} pos Source buffer offset
	 * @returns {number} Value read
	 */

	/**
	 * Reads a 64 bit double from a buffer using big endian byte order.
	 * @name util.float.readDoubleBE
	 * @function
	 * @param {Uint8Array} buf Source buffer
	 * @param {number} pos Source buffer offset
	 * @returns {number} Value read
	 */

	// Factory function for the purpose of node-based testing in modified global environments
	function factory(exports) {

	    // float: typed array
	    if (typeof Float32Array !== "undefined") (function() {

	        var f32 = new Float32Array([ -0 ]),
	            f8b = new Uint8Array(f32.buffer),
	            le  = f8b[3] === 128;

	        function writeFloat_f32_cpy(val, buf, pos) {
	            f32[0] = val;
	            buf[pos    ] = f8b[0];
	            buf[pos + 1] = f8b[1];
	            buf[pos + 2] = f8b[2];
	            buf[pos + 3] = f8b[3];
	        }

	        function writeFloat_f32_rev(val, buf, pos) {
	            f32[0] = val;
	            buf[pos    ] = f8b[3];
	            buf[pos + 1] = f8b[2];
	            buf[pos + 2] = f8b[1];
	            buf[pos + 3] = f8b[0];
	        }

	        /* istanbul ignore next */
	        exports.writeFloatLE = le ? writeFloat_f32_cpy : writeFloat_f32_rev;
	        /* istanbul ignore next */
	        exports.writeFloatBE = le ? writeFloat_f32_rev : writeFloat_f32_cpy;

	        function readFloat_f32_cpy(buf, pos) {
	            f8b[0] = buf[pos    ];
	            f8b[1] = buf[pos + 1];
	            f8b[2] = buf[pos + 2];
	            f8b[3] = buf[pos + 3];
	            return f32[0];
	        }

	        function readFloat_f32_rev(buf, pos) {
	            f8b[3] = buf[pos    ];
	            f8b[2] = buf[pos + 1];
	            f8b[1] = buf[pos + 2];
	            f8b[0] = buf[pos + 3];
	            return f32[0];
	        }

	        /* istanbul ignore next */
	        exports.readFloatLE = le ? readFloat_f32_cpy : readFloat_f32_rev;
	        /* istanbul ignore next */
	        exports.readFloatBE = le ? readFloat_f32_rev : readFloat_f32_cpy;

	    // float: ieee754
	    })(); else (function() {

	        function writeFloat_ieee754(writeUint, val, buf, pos) {
	            var sign = val < 0 ? 1 : 0;
	            if (sign)
	                val = -val;
	            if (val === 0)
	                writeUint(1 / val > 0 ? /* positive */ 0 : /* negative 0 */ 2147483648, buf, pos);
	            else if (isNaN(val))
	                writeUint(2143289344, buf, pos);
	            else if (val > 3.4028234663852886e+38) // +-Infinity
	                writeUint((sign << 31 | 2139095040) >>> 0, buf, pos);
	            else if (val < 1.1754943508222875e-38) // denormal
	                writeUint((sign << 31 | Math.round(val / 1.401298464324817e-45)) >>> 0, buf, pos);
	            else {
	                var exponent = Math.floor(Math.log(val) / Math.LN2),
	                    mantissa = Math.round(val * Math.pow(2, -exponent) * 8388608) & 8388607;
	                writeUint((sign << 31 | exponent + 127 << 23 | mantissa) >>> 0, buf, pos);
	            }
	        }

	        exports.writeFloatLE = writeFloat_ieee754.bind(null, writeUintLE);
	        exports.writeFloatBE = writeFloat_ieee754.bind(null, writeUintBE);

	        function readFloat_ieee754(readUint, buf, pos) {
	            var uint = readUint(buf, pos),
	                sign = (uint >> 31) * 2 + 1,
	                exponent = uint >>> 23 & 255,
	                mantissa = uint & 8388607;
	            return exponent === 255
	                ? mantissa
	                ? NaN
	                : sign * Infinity
	                : exponent === 0 // denormal
	                ? sign * 1.401298464324817e-45 * mantissa
	                : sign * Math.pow(2, exponent - 150) * (mantissa + 8388608);
	        }

	        exports.readFloatLE = readFloat_ieee754.bind(null, readUintLE);
	        exports.readFloatBE = readFloat_ieee754.bind(null, readUintBE);

	    })();

	    // double: typed array
	    if (typeof Float64Array !== "undefined") (function() {

	        var f64 = new Float64Array([-0]),
	            f8b = new Uint8Array(f64.buffer),
	            le  = f8b[7] === 128;

	        function writeDouble_f64_cpy(val, buf, pos) {
	            f64[0] = val;
	            buf[pos    ] = f8b[0];
	            buf[pos + 1] = f8b[1];
	            buf[pos + 2] = f8b[2];
	            buf[pos + 3] = f8b[3];
	            buf[pos + 4] = f8b[4];
	            buf[pos + 5] = f8b[5];
	            buf[pos + 6] = f8b[6];
	            buf[pos + 7] = f8b[7];
	        }

	        function writeDouble_f64_rev(val, buf, pos) {
	            f64[0] = val;
	            buf[pos    ] = f8b[7];
	            buf[pos + 1] = f8b[6];
	            buf[pos + 2] = f8b[5];
	            buf[pos + 3] = f8b[4];
	            buf[pos + 4] = f8b[3];
	            buf[pos + 5] = f8b[2];
	            buf[pos + 6] = f8b[1];
	            buf[pos + 7] = f8b[0];
	        }

	        /* istanbul ignore next */
	        exports.writeDoubleLE = le ? writeDouble_f64_cpy : writeDouble_f64_rev;
	        /* istanbul ignore next */
	        exports.writeDoubleBE = le ? writeDouble_f64_rev : writeDouble_f64_cpy;

	        function readDouble_f64_cpy(buf, pos) {
	            f8b[0] = buf[pos    ];
	            f8b[1] = buf[pos + 1];
	            f8b[2] = buf[pos + 2];
	            f8b[3] = buf[pos + 3];
	            f8b[4] = buf[pos + 4];
	            f8b[5] = buf[pos + 5];
	            f8b[6] = buf[pos + 6];
	            f8b[7] = buf[pos + 7];
	            return f64[0];
	        }

	        function readDouble_f64_rev(buf, pos) {
	            f8b[7] = buf[pos    ];
	            f8b[6] = buf[pos + 1];
	            f8b[5] = buf[pos + 2];
	            f8b[4] = buf[pos + 3];
	            f8b[3] = buf[pos + 4];
	            f8b[2] = buf[pos + 5];
	            f8b[1] = buf[pos + 6];
	            f8b[0] = buf[pos + 7];
	            return f64[0];
	        }

	        /* istanbul ignore next */
	        exports.readDoubleLE = le ? readDouble_f64_cpy : readDouble_f64_rev;
	        /* istanbul ignore next */
	        exports.readDoubleBE = le ? readDouble_f64_rev : readDouble_f64_cpy;

	    // double: ieee754
	    })(); else (function() {

	        function writeDouble_ieee754(writeUint, off0, off1, val, buf, pos) {
	            var sign = val < 0 ? 1 : 0;
	            if (sign)
	                val = -val;
	            if (val === 0) {
	                writeUint(0, buf, pos + off0);
	                writeUint(1 / val > 0 ? /* positive */ 0 : /* negative 0 */ 2147483648, buf, pos + off1);
	            } else if (isNaN(val)) {
	                writeUint(0, buf, pos + off0);
	                writeUint(2146959360, buf, pos + off1);
	            } else if (val > 1.7976931348623157e+308) { // +-Infinity
	                writeUint(0, buf, pos + off0);
	                writeUint((sign << 31 | 2146435072) >>> 0, buf, pos + off1);
	            } else {
	                var mantissa;
	                if (val < 2.2250738585072014e-308) { // denormal
	                    mantissa = val / 5e-324;
	                    writeUint(mantissa >>> 0, buf, pos + off0);
	                    writeUint((sign << 31 | mantissa / 4294967296) >>> 0, buf, pos + off1);
	                } else {
	                    var exponent = Math.floor(Math.log(val) / Math.LN2);
	                    if (exponent === 1024)
	                        exponent = 1023;
	                    mantissa = val * Math.pow(2, -exponent);
	                    writeUint(mantissa * 4503599627370496 >>> 0, buf, pos + off0);
	                    writeUint((sign << 31 | exponent + 1023 << 20 | mantissa * 1048576 & 1048575) >>> 0, buf, pos + off1);
	                }
	            }
	        }

	        exports.writeDoubleLE = writeDouble_ieee754.bind(null, writeUintLE, 0, 4);
	        exports.writeDoubleBE = writeDouble_ieee754.bind(null, writeUintBE, 4, 0);

	        function readDouble_ieee754(readUint, off0, off1, buf, pos) {
	            var lo = readUint(buf, pos + off0),
	                hi = readUint(buf, pos + off1);
	            var sign = (hi >> 31) * 2 + 1,
	                exponent = hi >>> 20 & 2047,
	                mantissa = 4294967296 * (hi & 1048575) + lo;
	            return exponent === 2047
	                ? mantissa
	                ? NaN
	                : sign * Infinity
	                : exponent === 0 // denormal
	                ? sign * 5e-324 * mantissa
	                : sign * Math.pow(2, exponent - 1075) * (mantissa + 4503599627370496);
	        }

	        exports.readDoubleLE = readDouble_ieee754.bind(null, readUintLE, 0, 4);
	        exports.readDoubleBE = readDouble_ieee754.bind(null, readUintBE, 4, 0);

	    })();

	    return exports;
	}

	// uint helpers

	function writeUintLE(val, buf, pos) {
	    buf[pos    ] =  val        & 255;
	    buf[pos + 1] =  val >>> 8  & 255;
	    buf[pos + 2] =  val >>> 16 & 255;
	    buf[pos + 3] =  val >>> 24;
	}

	function writeUintBE(val, buf, pos) {
	    buf[pos    ] =  val >>> 24;
	    buf[pos + 1] =  val >>> 16 & 255;
	    buf[pos + 2] =  val >>> 8  & 255;
	    buf[pos + 3] =  val        & 255;
	}

	function readUintLE(buf, pos) {
	    return (buf[pos    ]
	          | buf[pos + 1] << 8
	          | buf[pos + 2] << 16
	          | buf[pos + 3] << 24) >>> 0;
	}

	function readUintBE(buf, pos) {
	    return (buf[pos    ] << 24
	          | buf[pos + 1] << 16
	          | buf[pos + 2] << 8
	          | buf[pos + 3]) >>> 0;
	}
	return float;
}

var inquire_1;
var hasRequiredInquire;

function requireInquire () {
	if (hasRequiredInquire) return inquire_1;
	hasRequiredInquire = 1;
	inquire_1 = inquire;

	/**
	 * Requires a module only if available.
	 * @memberof util
	 * @param {string} moduleName Module to require
	 * @returns {?Object} Required module if available and not empty, otherwise `null`
	 */
	function inquire(moduleName) {
	    try {
	        var mod = eval("quire".replace(/^/,"re"))(moduleName); // eslint-disable-line no-eval
	        if (mod && (mod.length || Object.keys(mod).length))
	            return mod;
	    } catch (e) {} // eslint-disable-line no-empty
	    return null;
	}
	return inquire_1;
}

var utf8 = {};

var hasRequiredUtf8;

function requireUtf8 () {
	if (hasRequiredUtf8) return utf8;
	hasRequiredUtf8 = 1;
	(function (exports) {

		/**
		 * A minimal UTF8 implementation for number arrays.
		 * @memberof util
		 * @namespace
		 */
		var utf8 = exports;

		/**
		 * Calculates the UTF8 byte length of a string.
		 * @param {string} string String
		 * @returns {number} Byte length
		 */
		utf8.length = function utf8_length(string) {
		    var len = 0,
		        c = 0;
		    for (var i = 0; i < string.length; ++i) {
		        c = string.charCodeAt(i);
		        if (c < 128)
		            len += 1;
		        else if (c < 2048)
		            len += 2;
		        else if ((c & 0xFC00) === 0xD800 && (string.charCodeAt(i + 1) & 0xFC00) === 0xDC00) {
		            ++i;
		            len += 4;
		        } else
		            len += 3;
		    }
		    return len;
		};

		/**
		 * Reads UTF8 bytes as a string.
		 * @param {Uint8Array} buffer Source buffer
		 * @param {number} start Source start
		 * @param {number} end Source end
		 * @returns {string} String read
		 */
		utf8.read = function utf8_read(buffer, start, end) {
		    var len = end - start;
		    if (len < 1)
		        return "";
		    var parts = null,
		        chunk = [],
		        i = 0, // char offset
		        t;     // temporary
		    while (start < end) {
		        t = buffer[start++];
		        if (t < 128)
		            chunk[i++] = t;
		        else if (t > 191 && t < 224)
		            chunk[i++] = (t & 31) << 6 | buffer[start++] & 63;
		        else if (t > 239 && t < 365) {
		            t = ((t & 7) << 18 | (buffer[start++] & 63) << 12 | (buffer[start++] & 63) << 6 | buffer[start++] & 63) - 0x10000;
		            chunk[i++] = 0xD800 + (t >> 10);
		            chunk[i++] = 0xDC00 + (t & 1023);
		        } else
		            chunk[i++] = (t & 15) << 12 | (buffer[start++] & 63) << 6 | buffer[start++] & 63;
		        if (i > 8191) {
		            (parts || (parts = [])).push(String.fromCharCode.apply(String, chunk));
		            i = 0;
		        }
		    }
		    if (parts) {
		        if (i)
		            parts.push(String.fromCharCode.apply(String, chunk.slice(0, i)));
		        return parts.join("");
		    }
		    return String.fromCharCode.apply(String, chunk.slice(0, i));
		};

		/**
		 * Writes a string as UTF8 bytes.
		 * @param {string} string Source string
		 * @param {Uint8Array} buffer Destination buffer
		 * @param {number} offset Destination offset
		 * @returns {number} Bytes written
		 */
		utf8.write = function utf8_write(string, buffer, offset) {
		    var start = offset,
		        c1, // character 1
		        c2; // character 2
		    for (var i = 0; i < string.length; ++i) {
		        c1 = string.charCodeAt(i);
		        if (c1 < 128) {
		            buffer[offset++] = c1;
		        } else if (c1 < 2048) {
		            buffer[offset++] = c1 >> 6       | 192;
		            buffer[offset++] = c1       & 63 | 128;
		        } else if ((c1 & 0xFC00) === 0xD800 && ((c2 = string.charCodeAt(i + 1)) & 0xFC00) === 0xDC00) {
		            c1 = 0x10000 + ((c1 & 0x03FF) << 10) + (c2 & 0x03FF);
		            ++i;
		            buffer[offset++] = c1 >> 18      | 240;
		            buffer[offset++] = c1 >> 12 & 63 | 128;
		            buffer[offset++] = c1 >> 6  & 63 | 128;
		            buffer[offset++] = c1       & 63 | 128;
		        } else {
		            buffer[offset++] = c1 >> 12      | 224;
		            buffer[offset++] = c1 >> 6  & 63 | 128;
		            buffer[offset++] = c1       & 63 | 128;
		        }
		    }
		    return offset - start;
		};
} (utf8));
	return utf8;
}

var pool_1;
var hasRequiredPool;

function requirePool () {
	if (hasRequiredPool) return pool_1;
	hasRequiredPool = 1;
	pool_1 = pool;

	/**
	 * An allocator as used by {@link util.pool}.
	 * @typedef PoolAllocator
	 * @type {function}
	 * @param {number} size Buffer size
	 * @returns {Uint8Array} Buffer
	 */

	/**
	 * A slicer as used by {@link util.pool}.
	 * @typedef PoolSlicer
	 * @type {function}
	 * @param {number} start Start offset
	 * @param {number} end End offset
	 * @returns {Uint8Array} Buffer slice
	 * @this {Uint8Array}
	 */

	/**
	 * A general purpose buffer pool.
	 * @memberof util
	 * @function
	 * @param {PoolAllocator} alloc Allocator
	 * @param {PoolSlicer} slice Slicer
	 * @param {number} [size=8192] Slab size
	 * @returns {PoolAllocator} Pooled allocator
	 */
	function pool(alloc, slice, size) {
	    var SIZE   = size || 8192;
	    var MAX    = SIZE >>> 1;
	    var slab   = null;
	    var offset = SIZE;
	    return function pool_alloc(size) {
	        if (size < 1 || size > MAX)
	            return alloc(size);
	        if (offset + size > SIZE) {
	            slab = alloc(SIZE);
	            offset = 0;
	        }
	        var buf = slice.call(slab, offset, offset += size);
	        if (offset & 7) // align to 32 bit
	            offset = (offset | 7) + 1;
	        return buf;
	    };
	}
	return pool_1;
}

var longbits;
var hasRequiredLongbits;

function requireLongbits () {
	if (hasRequiredLongbits) return longbits;
	hasRequiredLongbits = 1;
	longbits = LongBits;

	var util = requireMinimal$1();

	/**
	 * Constructs new long bits.
	 * @classdesc Helper class for working with the low and high bits of a 64 bit value.
	 * @memberof util
	 * @constructor
	 * @param {number} lo Low 32 bits, unsigned
	 * @param {number} hi High 32 bits, unsigned
	 */
	function LongBits(lo, hi) {

	    // note that the casts below are theoretically unnecessary as of today, but older statically
	    // generated converter code might still call the ctor with signed 32bits. kept for compat.

	    /**
	     * Low bits.
	     * @type {number}
	     */
	    this.lo = lo >>> 0;

	    /**
	     * High bits.
	     * @type {number}
	     */
	    this.hi = hi >>> 0;
	}

	/**
	 * Zero bits.
	 * @memberof util.LongBits
	 * @type {util.LongBits}
	 */
	var zero = LongBits.zero = new LongBits(0, 0);

	zero.toNumber = function() { return 0; };
	zero.zzEncode = zero.zzDecode = function() { return this; };
	zero.length = function() { return 1; };

	/**
	 * Zero hash.
	 * @memberof util.LongBits
	 * @type {string}
	 */
	var zeroHash = LongBits.zeroHash = "\0\0\0\0\0\0\0\0";

	/**
	 * Constructs new long bits from the specified number.
	 * @param {number} value Value
	 * @returns {util.LongBits} Instance
	 */
	LongBits.fromNumber = function fromNumber(value) {
	    if (value === 0)
	        return zero;
	    var sign = value < 0;
	    if (sign)
	        value = -value;
	    var lo = value >>> 0,
	        hi = (value - lo) / 4294967296 >>> 0;
	    if (sign) {
	        hi = ~hi >>> 0;
	        lo = ~lo >>> 0;
	        if (++lo > 4294967295) {
	            lo = 0;
	            if (++hi > 4294967295)
	                hi = 0;
	        }
	    }
	    return new LongBits(lo, hi);
	};

	/**
	 * Constructs new long bits from a number, long or string.
	 * @param {Long|number|string} value Value
	 * @returns {util.LongBits} Instance
	 */
	LongBits.from = function from(value) {
	    if (typeof value === "number")
	        return LongBits.fromNumber(value);
	    if (util.isString(value)) {
	        /* istanbul ignore else */
	        if (util.Long)
	            value = util.Long.fromString(value);
	        else
	            return LongBits.fromNumber(parseInt(value, 10));
	    }
	    return value.low || value.high ? new LongBits(value.low >>> 0, value.high >>> 0) : zero;
	};

	/**
	 * Converts this long bits to a possibly unsafe JavaScript number.
	 * @param {boolean} [unsigned=false] Whether unsigned or not
	 * @returns {number} Possibly unsafe number
	 */
	LongBits.prototype.toNumber = function toNumber(unsigned) {
	    if (!unsigned && this.hi >>> 31) {
	        var lo = ~this.lo + 1 >>> 0,
	            hi = ~this.hi     >>> 0;
	        if (!lo)
	            hi = hi + 1 >>> 0;
	        return -(lo + hi * 4294967296);
	    }
	    return this.lo + this.hi * 4294967296;
	};

	/**
	 * Converts this long bits to a long.
	 * @param {boolean} [unsigned=false] Whether unsigned or not
	 * @returns {Long} Long
	 */
	LongBits.prototype.toLong = function toLong(unsigned) {
	    return util.Long
	        ? new util.Long(this.lo | 0, this.hi | 0, Boolean(unsigned))
	        /* istanbul ignore next */
	        : { low: this.lo | 0, high: this.hi | 0, unsigned: Boolean(unsigned) };
	};

	var charCodeAt = String.prototype.charCodeAt;

	/**
	 * Constructs new long bits from the specified 8 characters long hash.
	 * @param {string} hash Hash
	 * @returns {util.LongBits} Bits
	 */
	LongBits.fromHash = function fromHash(hash) {
	    if (hash === zeroHash)
	        return zero;
	    return new LongBits(
	        ( charCodeAt.call(hash, 0)
	        | charCodeAt.call(hash, 1) << 8
	        | charCodeAt.call(hash, 2) << 16
	        | charCodeAt.call(hash, 3) << 24) >>> 0
	    ,
	        ( charCodeAt.call(hash, 4)
	        | charCodeAt.call(hash, 5) << 8
	        | charCodeAt.call(hash, 6) << 16
	        | charCodeAt.call(hash, 7) << 24) >>> 0
	    );
	};

	/**
	 * Converts this long bits to a 8 characters long hash.
	 * @returns {string} Hash
	 */
	LongBits.prototype.toHash = function toHash() {
	    return String.fromCharCode(
	        this.lo        & 255,
	        this.lo >>> 8  & 255,
	        this.lo >>> 16 & 255,
	        this.lo >>> 24      ,
	        this.hi        & 255,
	        this.hi >>> 8  & 255,
	        this.hi >>> 16 & 255,
	        this.hi >>> 24
	    );
	};

	/**
	 * Zig-zag encodes this long bits.
	 * @returns {util.LongBits} `this`
	 */
	LongBits.prototype.zzEncode = function zzEncode() {
	    var mask =   this.hi >> 31;
	    this.hi  = ((this.hi << 1 | this.lo >>> 31) ^ mask) >>> 0;
	    this.lo  = ( this.lo << 1                   ^ mask) >>> 0;
	    return this;
	};

	/**
	 * Zig-zag decodes this long bits.
	 * @returns {util.LongBits} `this`
	 */
	LongBits.prototype.zzDecode = function zzDecode() {
	    var mask = -(this.lo & 1);
	    this.lo  = ((this.lo >>> 1 | this.hi << 31) ^ mask) >>> 0;
	    this.hi  = ( this.hi >>> 1                  ^ mask) >>> 0;
	    return this;
	};

	/**
	 * Calculates the length of this longbits when encoded as a varint.
	 * @returns {number} Length
	 */
	LongBits.prototype.length = function length() {
	    var part0 =  this.lo,
	        part1 = (this.lo >>> 28 | this.hi << 4) >>> 0,
	        part2 =  this.hi >>> 24;
	    return part2 === 0
	         ? part1 === 0
	           ? part0 < 16384
	             ? part0 < 128 ? 1 : 2
	             : part0 < 2097152 ? 3 : 4
	           : part1 < 16384
	             ? part1 < 128 ? 5 : 6
	             : part1 < 2097152 ? 7 : 8
	         : part2 < 128 ? 9 : 10;
	};
	return longbits;
}

var hasRequiredMinimal$1;

function requireMinimal$1 () {
	if (hasRequiredMinimal$1) return minimal;
	hasRequiredMinimal$1 = 1;
	(function (exports) {
		var util = exports;

		// used to return a Promise where callback is omitted
		util.asPromise = requireAspromise();

		// converts to / from base64 encoded strings
		util.base64 = requireBase64();

		// base class of rpc.Service
		util.EventEmitter = requireEventemitter();

		// float handling accross browsers
		util.float = requireFloat();

		// requires modules optionally and hides the call from bundlers
		util.inquire = requireInquire();

		// converts to / from utf8 encoded strings
		util.utf8 = requireUtf8();

		// provides a node-like buffer pool in the browser
		util.pool = requirePool();

		// utility to work with the low and high bits of a 64 bit value
		util.LongBits = requireLongbits();

		/**
		 * Whether running within node or not.
		 * @memberof util
		 * @type {boolean}
		 */
		util.isNode = Boolean(typeof commonjsGlobal !== "undefined"
		                   && commonjsGlobal
		                   && commonjsGlobal.process
		                   && commonjsGlobal.process.versions
		                   && commonjsGlobal.process.versions.node);

		/**
		 * Global object reference.
		 * @memberof util
		 * @type {Object}
		 */
		util.global = util.isNode && commonjsGlobal
		           || typeof undefined !== "undefined" 
		           || typeof self   !== "undefined" && self
		           || commonjsGlobal; // eslint-disable-line no-invalid-this

		/**
		 * An immuable empty array.
		 * @memberof util
		 * @type {Array.<*>}
		 * @const
		 */
		util.emptyArray = Object.freeze ? Object.freeze([]) : /* istanbul ignore next */ []; // used on prototypes

		/**
		 * An immutable empty object.
		 * @type {Object}
		 * @const
		 */
		util.emptyObject = Object.freeze ? Object.freeze({}) : /* istanbul ignore next */ {}; // used on prototypes

		/**
		 * Tests if the specified value is an integer.
		 * @function
		 * @param {*} value Value to test
		 * @returns {boolean} `true` if the value is an integer
		 */
		util.isInteger = Number.isInteger || /* istanbul ignore next */ function isInteger(value) {
		    return typeof value === "number" && isFinite(value) && Math.floor(value) === value;
		};

		/**
		 * Tests if the specified value is a string.
		 * @param {*} value Value to test
		 * @returns {boolean} `true` if the value is a string
		 */
		util.isString = function isString(value) {
		    return typeof value === "string" || value instanceof String;
		};

		/**
		 * Tests if the specified value is a non-null object.
		 * @param {*} value Value to test
		 * @returns {boolean} `true` if the value is a non-null object
		 */
		util.isObject = function isObject(value) {
		    return value && typeof value === "object";
		};

		/**
		 * Checks if a property on a message is considered to be present.
		 * This is an alias of {@link util.isSet}.
		 * @function
		 * @param {Object} obj Plain object or message instance
		 * @param {string} prop Property name
		 * @returns {boolean} `true` if considered to be present, otherwise `false`
		 */
		util.isset =

		/**
		 * Checks if a property on a message is considered to be present.
		 * @param {Object} obj Plain object or message instance
		 * @param {string} prop Property name
		 * @returns {boolean} `true` if considered to be present, otherwise `false`
		 */
		util.isSet = function isSet(obj, prop) {
		    var value = obj[prop];
		    if (value != null && obj.hasOwnProperty(prop)) // eslint-disable-line eqeqeq, no-prototype-builtins
		        return typeof value !== "object" || (Array.isArray(value) ? value.length : Object.keys(value).length) > 0;
		    return false;
		};

		/**
		 * Any compatible Buffer instance.
		 * This is a minimal stand-alone definition of a Buffer instance. The actual type is that exported by node's typings.
		 * @interface Buffer
		 * @extends Uint8Array
		 */

		/**
		 * Node's Buffer class if available.
		 * @type {Constructor<Buffer>}
		 */
		util.Buffer = (function() {
		    try {
		        var Buffer = util.inquire("buffer").Buffer;
		        // refuse to use non-node buffers if not explicitly assigned (perf reasons):
		        return Buffer.prototype.utf8Write ? Buffer : /* istanbul ignore next */ null;
		    } catch (e) {
		        /* istanbul ignore next */
		        return null;
		    }
		})();

		// Internal alias of or polyfull for Buffer.from.
		util._Buffer_from = null;

		// Internal alias of or polyfill for Buffer.allocUnsafe.
		util._Buffer_allocUnsafe = null;

		/**
		 * Creates a new buffer of whatever type supported by the environment.
		 * @param {number|number[]} [sizeOrArray=0] Buffer size or number array
		 * @returns {Uint8Array|Buffer} Buffer
		 */
		util.newBuffer = function newBuffer(sizeOrArray) {
		    /* istanbul ignore next */
		    return typeof sizeOrArray === "number"
		        ? util.Buffer
		            ? util._Buffer_allocUnsafe(sizeOrArray)
		            : new util.Array(sizeOrArray)
		        : util.Buffer
		            ? util._Buffer_from(sizeOrArray)
		            : typeof Uint8Array === "undefined"
		                ? sizeOrArray
		                : new Uint8Array(sizeOrArray);
		};

		/**
		 * Array implementation used in the browser. `Uint8Array` if supported, otherwise `Array`.
		 * @type {Constructor<Uint8Array>}
		 */
		util.Array = typeof Uint8Array !== "undefined" ? Uint8Array /* istanbul ignore next */ : Array;

		/**
		 * Any compatible Long instance.
		 * This is a minimal stand-alone definition of a Long instance. The actual type is that exported by long.js.
		 * @interface Long
		 * @property {number} low Low bits
		 * @property {number} high High bits
		 * @property {boolean} unsigned Whether unsigned or not
		 */

		/**
		 * Long.js's Long class if available.
		 * @type {Constructor<Long>}
		 */
		util.Long = /* istanbul ignore next */ util.global.dcodeIO && /* istanbul ignore next */ util.global.dcodeIO.Long
		         || /* istanbul ignore next */ util.global.Long
		         || util.inquire("long");

		/**
		 * Regular expression used to verify 2 bit (`bool`) map keys.
		 * @type {RegExp}
		 * @const
		 */
		util.key2Re = /^true|false|0|1$/;

		/**
		 * Regular expression used to verify 32 bit (`int32` etc.) map keys.
		 * @type {RegExp}
		 * @const
		 */
		util.key32Re = /^-?(?:0|[1-9][0-9]*)$/;

		/**
		 * Regular expression used to verify 64 bit (`int64` etc.) map keys.
		 * @type {RegExp}
		 * @const
		 */
		util.key64Re = /^(?:[\\x00-\\xff]{8}|-?(?:0|[1-9][0-9]*))$/;

		/**
		 * Converts a number or long to an 8 characters long hash string.
		 * @param {Long|number} value Value to convert
		 * @returns {string} Hash
		 */
		util.longToHash = function longToHash(value) {
		    return value
		        ? util.LongBits.from(value).toHash()
		        : util.LongBits.zeroHash;
		};

		/**
		 * Converts an 8 characters long hash string to a long or number.
		 * @param {string} hash Hash
		 * @param {boolean} [unsigned=false] Whether unsigned or not
		 * @returns {Long|number} Original value
		 */
		util.longFromHash = function longFromHash(hash, unsigned) {
		    var bits = util.LongBits.fromHash(hash);
		    if (util.Long)
		        return util.Long.fromBits(bits.lo, bits.hi, unsigned);
		    return bits.toNumber(Boolean(unsigned));
		};

		/**
		 * Merges the properties of the source object into the destination object.
		 * @memberof util
		 * @param {Object.<string,*>} dst Destination object
		 * @param {Object.<string,*>} src Source object
		 * @param {boolean} [ifNotSet=false] Merges only if the key is not already set
		 * @returns {Object.<string,*>} Destination object
		 */
		function merge(dst, src, ifNotSet) { // used by converters
		    for (var keys = Object.keys(src), i = 0; i < keys.length; ++i)
		        if (dst[keys[i]] === undefined || !ifNotSet)
		            dst[keys[i]] = src[keys[i]];
		    return dst;
		}

		util.merge = merge;

		/**
		 * Converts the first character of a string to lower case.
		 * @param {string} str String to convert
		 * @returns {string} Converted string
		 */
		util.lcFirst = function lcFirst(str) {
		    return str.charAt(0).toLowerCase() + str.substring(1);
		};

		/**
		 * Creates a custom error constructor.
		 * @memberof util
		 * @param {string} name Error name
		 * @returns {Constructor<Error>} Custom error constructor
		 */
		function newError(name) {

		    function CustomError(message, properties) {

		        if (!(this instanceof CustomError))
		            return new CustomError(message, properties);

		        // Error.call(this, message);
		        // ^ just returns a new error instance because the ctor can be called as a function

		        Object.defineProperty(this, "message", { get: function() { return message; } });

		        /* istanbul ignore next */
		        if (Error.captureStackTrace) // node
		            Error.captureStackTrace(this, CustomError);
		        else
		            Object.defineProperty(this, "stack", { value: new Error().stack || "" });

		        if (properties)
		            merge(this, properties);
		    }

		    (CustomError.prototype = Object.create(Error.prototype)).constructor = CustomError;

		    Object.defineProperty(CustomError.prototype, "name", { get: function() { return name; } });

		    CustomError.prototype.toString = function toString() {
		        return this.name + ": " + this.message;
		    };

		    return CustomError;
		}

		util.newError = newError;

		/**
		 * Constructs a new protocol error.
		 * @classdesc Error subclass indicating a protocol specifc error.
		 * @memberof util
		 * @extends Error
		 * @template T extends Message<T>
		 * @constructor
		 * @param {string} message Error message
		 * @param {Object.<string,*>} [properties] Additional properties
		 * @example
		 * try {
		 *     MyMessage.decode(someBuffer); // throws if required fields are missing
		 * } catch (e) {
		 *     if (e instanceof ProtocolError && e.instance)
		 *         console.log("decoded so far: " + JSON.stringify(e.instance));
		 * }
		 */
		util.ProtocolError = newError("ProtocolError");

		/**
		 * So far decoded message instance.
		 * @name util.ProtocolError#instance
		 * @type {Message<T>}
		 */

		/**
		 * A OneOf getter as returned by {@link util.oneOfGetter}.
		 * @typedef OneOfGetter
		 * @type {function}
		 * @returns {string|undefined} Set field name, if any
		 */

		/**
		 * Builds a getter for a oneof's present field name.
		 * @param {string[]} fieldNames Field names
		 * @returns {OneOfGetter} Unbound getter
		 */
		util.oneOfGetter = function getOneOf(fieldNames) {
		    var fieldMap = {};
		    for (var i = 0; i < fieldNames.length; ++i)
		        fieldMap[fieldNames[i]] = 1;

		    /**
		     * @returns {string|undefined} Set field name, if any
		     * @this Object
		     * @ignore
		     */
		    return function() { // eslint-disable-line consistent-return
		        for (var keys = Object.keys(this), i = keys.length - 1; i > -1; --i)
		            if (fieldMap[keys[i]] === 1 && this[keys[i]] !== undefined && this[keys[i]] !== null)
		                return keys[i];
		    };
		};

		/**
		 * A OneOf setter as returned by {@link util.oneOfSetter}.
		 * @typedef OneOfSetter
		 * @type {function}
		 * @param {string|undefined} value Field name
		 * @returns {undefined}
		 */

		/**
		 * Builds a setter for a oneof's present field name.
		 * @param {string[]} fieldNames Field names
		 * @returns {OneOfSetter} Unbound setter
		 */
		util.oneOfSetter = function setOneOf(fieldNames) {

		    /**
		     * @param {string} name Field name
		     * @returns {undefined}
		     * @this Object
		     * @ignore
		     */
		    return function(name) {
		        for (var i = 0; i < fieldNames.length; ++i)
		            if (fieldNames[i] !== name)
		                delete this[fieldNames[i]];
		    };
		};

		/**
		 * Default conversion options used for {@link Message#toJSON} implementations.
		 *
		 * These options are close to proto3's JSON mapping with the exception that internal types like Any are handled just like messages. More precisely:
		 *
		 * - Longs become strings
		 * - Enums become string keys
		 * - Bytes become base64 encoded strings
		 * - (Sub-)Messages become plain objects
		 * - Maps become plain objects with all string keys
		 * - Repeated fields become arrays
		 * - NaN and Infinity for float and double fields become strings
		 *
		 * @type {IConversionOptions}
		 * @see https://developers.google.com/protocol-buffers/docs/proto3?hl=en#json
		 */
		util.toJSONOptions = {
		    longs: String,
		    enums: String,
		    bytes: String,
		    json: true
		};

		// Sets up buffer utility according to the environment (called in index-minimal)
		util._configure = function() {
		    var Buffer = util.Buffer;
		    /* istanbul ignore if */
		    if (!Buffer) {
		        util._Buffer_from = util._Buffer_allocUnsafe = null;
		        return;
		    }
		    // because node 4.x buffers are incompatible & immutable
		    // see: https://github.com/dcodeIO/protobuf.js/pull/665
		    util._Buffer_from = Buffer.from !== Uint8Array.from && Buffer.from ||
		        /* istanbul ignore next */
		        function Buffer_from(value, encoding) {
		            return new Buffer(value, encoding);
		        };
		    util._Buffer_allocUnsafe = Buffer.allocUnsafe ||
		        /* istanbul ignore next */
		        function Buffer_allocUnsafe(size) {
		            return new Buffer(size);
		        };
		};
} (minimal));
	return minimal;
}

var writer;
var hasRequiredWriter;

function requireWriter () {
	if (hasRequiredWriter) return writer;
	hasRequiredWriter = 1;
	writer = Writer;

	var util      = requireMinimal$1();

	var BufferWriter; // cyclic

	var LongBits  = util.LongBits,
	    base64    = util.base64,
	    utf8      = util.utf8;

	/**
	 * Constructs a new writer operation instance.
	 * @classdesc Scheduled writer operation.
	 * @constructor
	 * @param {function(*, Uint8Array, number)} fn Function to call
	 * @param {number} len Value byte length
	 * @param {*} val Value to write
	 * @ignore
	 */
	function Op(fn, len, val) {

	    /**
	     * Function to call.
	     * @type {function(Uint8Array, number, *)}
	     */
	    this.fn = fn;

	    /**
	     * Value byte length.
	     * @type {number}
	     */
	    this.len = len;

	    /**
	     * Next operation.
	     * @type {Writer.Op|undefined}
	     */
	    this.next = undefined;

	    /**
	     * Value to write.
	     * @type {*}
	     */
	    this.val = val; // type varies
	}

	/* istanbul ignore next */
	function noop() {} // eslint-disable-line no-empty-function

	/**
	 * Constructs a new writer state instance.
	 * @classdesc Copied writer state.
	 * @memberof Writer
	 * @constructor
	 * @param {Writer} writer Writer to copy state from
	 * @ignore
	 */
	function State(writer) {

	    /**
	     * Current head.
	     * @type {Writer.Op}
	     */
	    this.head = writer.head;

	    /**
	     * Current tail.
	     * @type {Writer.Op}
	     */
	    this.tail = writer.tail;

	    /**
	     * Current buffer length.
	     * @type {number}
	     */
	    this.len = writer.len;

	    /**
	     * Next state.
	     * @type {State|null}
	     */
	    this.next = writer.states;
	}

	/**
	 * Constructs a new writer instance.
	 * @classdesc Wire format writer using `Uint8Array` if available, otherwise `Array`.
	 * @constructor
	 */
	function Writer() {

	    /**
	     * Current length.
	     * @type {number}
	     */
	    this.len = 0;

	    /**
	     * Operations head.
	     * @type {Object}
	     */
	    this.head = new Op(noop, 0, 0);

	    /**
	     * Operations tail
	     * @type {Object}
	     */
	    this.tail = this.head;

	    /**
	     * Linked forked states.
	     * @type {Object|null}
	     */
	    this.states = null;

	    // When a value is written, the writer calculates its byte length and puts it into a linked
	    // list of operations to perform when finish() is called. This both allows us to allocate
	    // buffers of the exact required size and reduces the amount of work we have to do compared
	    // to first calculating over objects and then encoding over objects. In our case, the encoding
	    // part is just a linked list walk calling operations with already prepared values.
	}

	var create = function create() {
	    return util.Buffer
	        ? function create_buffer_setup() {
	            return (Writer.create = function create_buffer() {
	                return new BufferWriter();
	            })();
	        }
	        /* istanbul ignore next */
	        : function create_array() {
	            return new Writer();
	        };
	};

	/**
	 * Creates a new writer.
	 * @function
	 * @returns {BufferWriter|Writer} A {@link BufferWriter} when Buffers are supported, otherwise a {@link Writer}
	 */
	Writer.create = create();

	/**
	 * Allocates a buffer of the specified size.
	 * @param {number} size Buffer size
	 * @returns {Uint8Array} Buffer
	 */
	Writer.alloc = function alloc(size) {
	    return new util.Array(size);
	};

	// Use Uint8Array buffer pool in the browser, just like node does with buffers
	/* istanbul ignore else */
	if (util.Array !== Array)
	    Writer.alloc = util.pool(Writer.alloc, util.Array.prototype.subarray);

	/**
	 * Pushes a new operation to the queue.
	 * @param {function(Uint8Array, number, *)} fn Function to call
	 * @param {number} len Value byte length
	 * @param {number} val Value to write
	 * @returns {Writer} `this`
	 * @private
	 */
	Writer.prototype._push = function push(fn, len, val) {
	    this.tail = this.tail.next = new Op(fn, len, val);
	    this.len += len;
	    return this;
	};

	function writeByte(val, buf, pos) {
	    buf[pos] = val & 255;
	}

	function writeVarint32(val, buf, pos) {
	    while (val > 127) {
	        buf[pos++] = val & 127 | 128;
	        val >>>= 7;
	    }
	    buf[pos] = val;
	}

	/**
	 * Constructs a new varint writer operation instance.
	 * @classdesc Scheduled varint writer operation.
	 * @extends Op
	 * @constructor
	 * @param {number} len Value byte length
	 * @param {number} val Value to write
	 * @ignore
	 */
	function VarintOp(len, val) {
	    this.len = len;
	    this.next = undefined;
	    this.val = val;
	}

	VarintOp.prototype = Object.create(Op.prototype);
	VarintOp.prototype.fn = writeVarint32;

	/**
	 * Writes an unsigned 32 bit value as a varint.
	 * @param {number} value Value to write
	 * @returns {Writer} `this`
	 */
	Writer.prototype.uint32 = function write_uint32(value) {
	    // here, the call to this.push has been inlined and a varint specific Op subclass is used.
	    // uint32 is by far the most frequently used operation and benefits significantly from this.
	    this.len += (this.tail = this.tail.next = new VarintOp(
	        (value = value >>> 0)
	                < 128       ? 1
	        : value < 16384     ? 2
	        : value < 2097152   ? 3
	        : value < 268435456 ? 4
	        :                     5,
	    value)).len;
	    return this;
	};

	/**
	 * Writes a signed 32 bit value as a varint.
	 * @function
	 * @param {number} value Value to write
	 * @returns {Writer} `this`
	 */
	Writer.prototype.int32 = function write_int32(value) {
	    return value < 0
	        ? this._push(writeVarint64, 10, LongBits.fromNumber(value)) // 10 bytes per spec
	        : this.uint32(value);
	};

	/**
	 * Writes a 32 bit value as a varint, zig-zag encoded.
	 * @param {number} value Value to write
	 * @returns {Writer} `this`
	 */
	Writer.prototype.sint32 = function write_sint32(value) {
	    return this.uint32((value << 1 ^ value >> 31) >>> 0);
	};

	function writeVarint64(val, buf, pos) {
	    while (val.hi) {
	        buf[pos++] = val.lo & 127 | 128;
	        val.lo = (val.lo >>> 7 | val.hi << 25) >>> 0;
	        val.hi >>>= 7;
	    }
	    while (val.lo > 127) {
	        buf[pos++] = val.lo & 127 | 128;
	        val.lo = val.lo >>> 7;
	    }
	    buf[pos++] = val.lo;
	}

	/**
	 * Writes an unsigned 64 bit value as a varint.
	 * @param {Long|number|string} value Value to write
	 * @returns {Writer} `this`
	 * @throws {TypeError} If `value` is a string and no long library is present.
	 */
	Writer.prototype.uint64 = function write_uint64(value) {
	    var bits = LongBits.from(value);
	    return this._push(writeVarint64, bits.length(), bits);
	};

	/**
	 * Writes a signed 64 bit value as a varint.
	 * @function
	 * @param {Long|number|string} value Value to write
	 * @returns {Writer} `this`
	 * @throws {TypeError} If `value` is a string and no long library is present.
	 */
	Writer.prototype.int64 = Writer.prototype.uint64;

	/**
	 * Writes a signed 64 bit value as a varint, zig-zag encoded.
	 * @param {Long|number|string} value Value to write
	 * @returns {Writer} `this`
	 * @throws {TypeError} If `value` is a string and no long library is present.
	 */
	Writer.prototype.sint64 = function write_sint64(value) {
	    var bits = LongBits.from(value).zzEncode();
	    return this._push(writeVarint64, bits.length(), bits);
	};

	/**
	 * Writes a boolish value as a varint.
	 * @param {boolean} value Value to write
	 * @returns {Writer} `this`
	 */
	Writer.prototype.bool = function write_bool(value) {
	    return this._push(writeByte, 1, value ? 1 : 0);
	};

	function writeFixed32(val, buf, pos) {
	    buf[pos    ] =  val         & 255;
	    buf[pos + 1] =  val >>> 8   & 255;
	    buf[pos + 2] =  val >>> 16  & 255;
	    buf[pos + 3] =  val >>> 24;
	}

	/**
	 * Writes an unsigned 32 bit value as fixed 32 bits.
	 * @param {number} value Value to write
	 * @returns {Writer} `this`
	 */
	Writer.prototype.fixed32 = function write_fixed32(value) {
	    return this._push(writeFixed32, 4, value >>> 0);
	};

	/**
	 * Writes a signed 32 bit value as fixed 32 bits.
	 * @function
	 * @param {number} value Value to write
	 * @returns {Writer} `this`
	 */
	Writer.prototype.sfixed32 = Writer.prototype.fixed32;

	/**
	 * Writes an unsigned 64 bit value as fixed 64 bits.
	 * @param {Long|number|string} value Value to write
	 * @returns {Writer} `this`
	 * @throws {TypeError} If `value` is a string and no long library is present.
	 */
	Writer.prototype.fixed64 = function write_fixed64(value) {
	    var bits = LongBits.from(value);
	    return this._push(writeFixed32, 4, bits.lo)._push(writeFixed32, 4, bits.hi);
	};

	/**
	 * Writes a signed 64 bit value as fixed 64 bits.
	 * @function
	 * @param {Long|number|string} value Value to write
	 * @returns {Writer} `this`
	 * @throws {TypeError} If `value` is a string and no long library is present.
	 */
	Writer.prototype.sfixed64 = Writer.prototype.fixed64;

	/**
	 * Writes a float (32 bit).
	 * @function
	 * @param {number} value Value to write
	 * @returns {Writer} `this`
	 */
	Writer.prototype.float = function write_float(value) {
	    return this._push(util.float.writeFloatLE, 4, value);
	};

	/**
	 * Writes a double (64 bit float).
	 * @function
	 * @param {number} value Value to write
	 * @returns {Writer} `this`
	 */
	Writer.prototype.double = function write_double(value) {
	    return this._push(util.float.writeDoubleLE, 8, value);
	};

	var writeBytes = util.Array.prototype.set
	    ? function writeBytes_set(val, buf, pos) {
	        buf.set(val, pos); // also works for plain array values
	    }
	    /* istanbul ignore next */
	    : function writeBytes_for(val, buf, pos) {
	        for (var i = 0; i < val.length; ++i)
	            buf[pos + i] = val[i];
	    };

	/**
	 * Writes a sequence of bytes.
	 * @param {Uint8Array|string} value Buffer or base64 encoded string to write
	 * @returns {Writer} `this`
	 */
	Writer.prototype.bytes = function write_bytes(value) {
	    var len = value.length >>> 0;
	    if (!len)
	        return this._push(writeByte, 1, 0);
	    if (util.isString(value)) {
	        var buf = Writer.alloc(len = base64.length(value));
	        base64.decode(value, buf, 0);
	        value = buf;
	    }
	    return this.uint32(len)._push(writeBytes, len, value);
	};

	/**
	 * Writes a string.
	 * @param {string} value Value to write
	 * @returns {Writer} `this`
	 */
	Writer.prototype.string = function write_string(value) {
	    var len = utf8.length(value);
	    return len
	        ? this.uint32(len)._push(utf8.write, len, value)
	        : this._push(writeByte, 1, 0);
	};

	/**
	 * Forks this writer's state by pushing it to a stack.
	 * Calling {@link Writer#reset|reset} or {@link Writer#ldelim|ldelim} resets the writer to the previous state.
	 * @returns {Writer} `this`
	 */
	Writer.prototype.fork = function fork() {
	    this.states = new State(this);
	    this.head = this.tail = new Op(noop, 0, 0);
	    this.len = 0;
	    return this;
	};

	/**
	 * Resets this instance to the last state.
	 * @returns {Writer} `this`
	 */
	Writer.prototype.reset = function reset() {
	    if (this.states) {
	        this.head   = this.states.head;
	        this.tail   = this.states.tail;
	        this.len    = this.states.len;
	        this.states = this.states.next;
	    } else {
	        this.head = this.tail = new Op(noop, 0, 0);
	        this.len  = 0;
	    }
	    return this;
	};

	/**
	 * Resets to the last state and appends the fork state's current write length as a varint followed by its operations.
	 * @returns {Writer} `this`
	 */
	Writer.prototype.ldelim = function ldelim() {
	    var head = this.head,
	        tail = this.tail,
	        len  = this.len;
	    this.reset().uint32(len);
	    if (len) {
	        this.tail.next = head.next; // skip noop
	        this.tail = tail;
	        this.len += len;
	    }
	    return this;
	};

	/**
	 * Finishes the write operation.
	 * @returns {Uint8Array} Finished buffer
	 */
	Writer.prototype.finish = function finish() {
	    var head = this.head.next, // skip noop
	        buf  = this.constructor.alloc(this.len),
	        pos  = 0;
	    while (head) {
	        head.fn(head.val, buf, pos);
	        pos += head.len;
	        head = head.next;
	    }
	    // this.head = this.tail = null;
	    return buf;
	};

	Writer._configure = function(BufferWriter_) {
	    BufferWriter = BufferWriter_;
	    Writer.create = create();
	    BufferWriter._configure();
	};
	return writer;
}

var writer_buffer;
var hasRequiredWriter_buffer;

function requireWriter_buffer () {
	if (hasRequiredWriter_buffer) return writer_buffer;
	hasRequiredWriter_buffer = 1;
	writer_buffer = BufferWriter;

	// extends Writer
	var Writer = requireWriter();
	(BufferWriter.prototype = Object.create(Writer.prototype)).constructor = BufferWriter;

	var util = requireMinimal$1();

	/**
	 * Constructs a new buffer writer instance.
	 * @classdesc Wire format writer using node buffers.
	 * @extends Writer
	 * @constructor
	 */
	function BufferWriter() {
	    Writer.call(this);
	}

	BufferWriter._configure = function () {
	    /**
	     * Allocates a buffer of the specified size.
	     * @function
	     * @param {number} size Buffer size
	     * @returns {Buffer} Buffer
	     */
	    BufferWriter.alloc = util._Buffer_allocUnsafe;

	    BufferWriter.writeBytesBuffer = util.Buffer && util.Buffer.prototype instanceof Uint8Array && util.Buffer.prototype.set.name === "set"
	        ? function writeBytesBuffer_set(val, buf, pos) {
	          buf.set(val, pos); // faster than copy (requires node >= 4 where Buffers extend Uint8Array and set is properly inherited)
	          // also works for plain array values
	        }
	        /* istanbul ignore next */
	        : function writeBytesBuffer_copy(val, buf, pos) {
	          if (val.copy) // Buffer values
	            val.copy(buf, pos, 0, val.length);
	          else for (var i = 0; i < val.length;) // plain array values
	            buf[pos++] = val[i++];
	        };
	};


	/**
	 * @override
	 */
	BufferWriter.prototype.bytes = function write_bytes_buffer(value) {
	    if (util.isString(value))
	        value = util._Buffer_from(value, "base64");
	    var len = value.length >>> 0;
	    this.uint32(len);
	    if (len)
	        this._push(BufferWriter.writeBytesBuffer, len, value);
	    return this;
	};

	function writeStringBuffer(val, buf, pos) {
	    if (val.length < 40) // plain js is faster for short strings (probably due to redundant assertions)
	        util.utf8.write(val, buf, pos);
	    else if (buf.utf8Write)
	        buf.utf8Write(val, pos);
	    else
	        buf.write(val, pos);
	}

	/**
	 * @override
	 */
	BufferWriter.prototype.string = function write_string_buffer(value) {
	    var len = util.Buffer.byteLength(value);
	    this.uint32(len);
	    if (len)
	        this._push(writeStringBuffer, len, value);
	    return this;
	};


	/**
	 * Finishes the write operation.
	 * @name BufferWriter#finish
	 * @function
	 * @returns {Buffer} Finished buffer
	 */

	BufferWriter._configure();
	return writer_buffer;
}

var reader;
var hasRequiredReader;

function requireReader () {
	if (hasRequiredReader) return reader;
	hasRequiredReader = 1;
	reader = Reader;

	var util      = requireMinimal$1();

	var BufferReader; // cyclic

	var LongBits  = util.LongBits,
	    utf8      = util.utf8;

	/* istanbul ignore next */
	function indexOutOfRange(reader, writeLength) {
	    return RangeError("index out of range: " + reader.pos + " + " + (writeLength || 1) + " > " + reader.len);
	}

	/**
	 * Constructs a new reader instance using the specified buffer.
	 * @classdesc Wire format reader using `Uint8Array` if available, otherwise `Array`.
	 * @constructor
	 * @param {Uint8Array} buffer Buffer to read from
	 */
	function Reader(buffer) {

	    /**
	     * Read buffer.
	     * @type {Uint8Array}
	     */
	    this.buf = buffer;

	    /**
	     * Read buffer position.
	     * @type {number}
	     */
	    this.pos = 0;

	    /**
	     * Read buffer length.
	     * @type {number}
	     */
	    this.len = buffer.length;
	}

	var create_array = typeof Uint8Array !== "undefined"
	    ? function create_typed_array(buffer) {
	        if (buffer instanceof Uint8Array || Array.isArray(buffer))
	            return new Reader(buffer);
	        throw Error("illegal buffer");
	    }
	    /* istanbul ignore next */
	    : function create_array(buffer) {
	        if (Array.isArray(buffer))
	            return new Reader(buffer);
	        throw Error("illegal buffer");
	    };

	var create = function create() {
	    return util.Buffer
	        ? function create_buffer_setup(buffer) {
	            return (Reader.create = function create_buffer(buffer) {
	                return util.Buffer.isBuffer(buffer)
	                    ? new BufferReader(buffer)
	                    /* istanbul ignore next */
	                    : create_array(buffer);
	            })(buffer);
	        }
	        /* istanbul ignore next */
	        : create_array;
	};

	/**
	 * Creates a new reader using the specified buffer.
	 * @function
	 * @param {Uint8Array|Buffer} buffer Buffer to read from
	 * @returns {Reader|BufferReader} A {@link BufferReader} if `buffer` is a Buffer, otherwise a {@link Reader}
	 * @throws {Error} If `buffer` is not a valid buffer
	 */
	Reader.create = create();

	Reader.prototype._slice = util.Array.prototype.subarray || /* istanbul ignore next */ util.Array.prototype.slice;

	/**
	 * Reads a varint as an unsigned 32 bit value.
	 * @function
	 * @returns {number} Value read
	 */
	Reader.prototype.uint32 = (function read_uint32_setup() {
	    var value = 4294967295; // optimizer type-hint, tends to deopt otherwise (?!)
	    return function read_uint32() {
	        value = (         this.buf[this.pos] & 127       ) >>> 0; if (this.buf[this.pos++] < 128) return value;
	        value = (value | (this.buf[this.pos] & 127) <<  7) >>> 0; if (this.buf[this.pos++] < 128) return value;
	        value = (value | (this.buf[this.pos] & 127) << 14) >>> 0; if (this.buf[this.pos++] < 128) return value;
	        value = (value | (this.buf[this.pos] & 127) << 21) >>> 0; if (this.buf[this.pos++] < 128) return value;
	        value = (value | (this.buf[this.pos] &  15) << 28) >>> 0; if (this.buf[this.pos++] < 128) return value;

	        /* istanbul ignore if */
	        if ((this.pos += 5) > this.len) {
	            this.pos = this.len;
	            throw indexOutOfRange(this, 10);
	        }
	        return value;
	    };
	})();

	/**
	 * Reads a varint as a signed 32 bit value.
	 * @returns {number} Value read
	 */
	Reader.prototype.int32 = function read_int32() {
	    return this.uint32() | 0;
	};

	/**
	 * Reads a zig-zag encoded varint as a signed 32 bit value.
	 * @returns {number} Value read
	 */
	Reader.prototype.sint32 = function read_sint32() {
	    var value = this.uint32();
	    return value >>> 1 ^ -(value & 1) | 0;
	};

	/* eslint-disable no-invalid-this */

	function readLongVarint() {
	    // tends to deopt with local vars for octet etc.
	    var bits = new LongBits(0, 0);
	    var i = 0;
	    if (this.len - this.pos > 4) { // fast route (lo)
	        for (; i < 4; ++i) {
	            // 1st..4th
	            bits.lo = (bits.lo | (this.buf[this.pos] & 127) << i * 7) >>> 0;
	            if (this.buf[this.pos++] < 128)
	                return bits;
	        }
	        // 5th
	        bits.lo = (bits.lo | (this.buf[this.pos] & 127) << 28) >>> 0;
	        bits.hi = (bits.hi | (this.buf[this.pos] & 127) >>  4) >>> 0;
	        if (this.buf[this.pos++] < 128)
	            return bits;
	        i = 0;
	    } else {
	        for (; i < 3; ++i) {
	            /* istanbul ignore if */
	            if (this.pos >= this.len)
	                throw indexOutOfRange(this);
	            // 1st..3th
	            bits.lo = (bits.lo | (this.buf[this.pos] & 127) << i * 7) >>> 0;
	            if (this.buf[this.pos++] < 128)
	                return bits;
	        }
	        // 4th
	        bits.lo = (bits.lo | (this.buf[this.pos++] & 127) << i * 7) >>> 0;
	        return bits;
	    }
	    if (this.len - this.pos > 4) { // fast route (hi)
	        for (; i < 5; ++i) {
	            // 6th..10th
	            bits.hi = (bits.hi | (this.buf[this.pos] & 127) << i * 7 + 3) >>> 0;
	            if (this.buf[this.pos++] < 128)
	                return bits;
	        }
	    } else {
	        for (; i < 5; ++i) {
	            /* istanbul ignore if */
	            if (this.pos >= this.len)
	                throw indexOutOfRange(this);
	            // 6th..10th
	            bits.hi = (bits.hi | (this.buf[this.pos] & 127) << i * 7 + 3) >>> 0;
	            if (this.buf[this.pos++] < 128)
	                return bits;
	        }
	    }
	    /* istanbul ignore next */
	    throw Error("invalid varint encoding");
	}

	/* eslint-enable no-invalid-this */

	/**
	 * Reads a varint as a signed 64 bit value.
	 * @name Reader#int64
	 * @function
	 * @returns {Long} Value read
	 */

	/**
	 * Reads a varint as an unsigned 64 bit value.
	 * @name Reader#uint64
	 * @function
	 * @returns {Long} Value read
	 */

	/**
	 * Reads a zig-zag encoded varint as a signed 64 bit value.
	 * @name Reader#sint64
	 * @function
	 * @returns {Long} Value read
	 */

	/**
	 * Reads a varint as a boolean.
	 * @returns {boolean} Value read
	 */
	Reader.prototype.bool = function read_bool() {
	    return this.uint32() !== 0;
	};

	function readFixed32_end(buf, end) { // note that this uses `end`, not `pos`
	    return (buf[end - 4]
	          | buf[end - 3] << 8
	          | buf[end - 2] << 16
	          | buf[end - 1] << 24) >>> 0;
	}

	/**
	 * Reads fixed 32 bits as an unsigned 32 bit integer.
	 * @returns {number} Value read
	 */
	Reader.prototype.fixed32 = function read_fixed32() {

	    /* istanbul ignore if */
	    if (this.pos + 4 > this.len)
	        throw indexOutOfRange(this, 4);

	    return readFixed32_end(this.buf, this.pos += 4);
	};

	/**
	 * Reads fixed 32 bits as a signed 32 bit integer.
	 * @returns {number} Value read
	 */
	Reader.prototype.sfixed32 = function read_sfixed32() {

	    /* istanbul ignore if */
	    if (this.pos + 4 > this.len)
	        throw indexOutOfRange(this, 4);

	    return readFixed32_end(this.buf, this.pos += 4) | 0;
	};

	/* eslint-disable no-invalid-this */

	function readFixed64(/* this: Reader */) {

	    /* istanbul ignore if */
	    if (this.pos + 8 > this.len)
	        throw indexOutOfRange(this, 8);

	    return new LongBits(readFixed32_end(this.buf, this.pos += 4), readFixed32_end(this.buf, this.pos += 4));
	}

	/* eslint-enable no-invalid-this */

	/**
	 * Reads fixed 64 bits.
	 * @name Reader#fixed64
	 * @function
	 * @returns {Long} Value read
	 */

	/**
	 * Reads zig-zag encoded fixed 64 bits.
	 * @name Reader#sfixed64
	 * @function
	 * @returns {Long} Value read
	 */

	/**
	 * Reads a float (32 bit) as a number.
	 * @function
	 * @returns {number} Value read
	 */
	Reader.prototype.float = function read_float() {

	    /* istanbul ignore if */
	    if (this.pos + 4 > this.len)
	        throw indexOutOfRange(this, 4);

	    var value = util.float.readFloatLE(this.buf, this.pos);
	    this.pos += 4;
	    return value;
	};

	/**
	 * Reads a double (64 bit float) as a number.
	 * @function
	 * @returns {number} Value read
	 */
	Reader.prototype.double = function read_double() {

	    /* istanbul ignore if */
	    if (this.pos + 8 > this.len)
	        throw indexOutOfRange(this, 4);

	    var value = util.float.readDoubleLE(this.buf, this.pos);
	    this.pos += 8;
	    return value;
	};

	/**
	 * Reads a sequence of bytes preceeded by its length as a varint.
	 * @returns {Uint8Array} Value read
	 */
	Reader.prototype.bytes = function read_bytes() {
	    var length = this.uint32(),
	        start  = this.pos,
	        end    = this.pos + length;

	    /* istanbul ignore if */
	    if (end > this.len)
	        throw indexOutOfRange(this, length);

	    this.pos += length;
	    if (Array.isArray(this.buf)) // plain array
	        return this.buf.slice(start, end);
	    return start === end // fix for IE 10/Win8 and others' subarray returning array of size 1
	        ? new this.buf.constructor(0)
	        : this._slice.call(this.buf, start, end);
	};

	/**
	 * Reads a string preceeded by its byte length as a varint.
	 * @returns {string} Value read
	 */
	Reader.prototype.string = function read_string() {
	    var bytes = this.bytes();
	    return utf8.read(bytes, 0, bytes.length);
	};

	/**
	 * Skips the specified number of bytes if specified, otherwise skips a varint.
	 * @param {number} [length] Length if known, otherwise a varint is assumed
	 * @returns {Reader} `this`
	 */
	Reader.prototype.skip = function skip(length) {
	    if (typeof length === "number") {
	        /* istanbul ignore if */
	        if (this.pos + length > this.len)
	            throw indexOutOfRange(this, length);
	        this.pos += length;
	    } else {
	        do {
	            /* istanbul ignore if */
	            if (this.pos >= this.len)
	                throw indexOutOfRange(this);
	        } while (this.buf[this.pos++] & 128);
	    }
	    return this;
	};

	/**
	 * Skips the next element of the specified wire type.
	 * @param {number} wireType Wire type received
	 * @returns {Reader} `this`
	 */
	Reader.prototype.skipType = function(wireType) {
	    switch (wireType) {
	        case 0:
	            this.skip();
	            break;
	        case 1:
	            this.skip(8);
	            break;
	        case 2:
	            this.skip(this.uint32());
	            break;
	        case 3:
	            while ((wireType = this.uint32() & 7) !== 4) {
	                this.skipType(wireType);
	            }
	            break;
	        case 5:
	            this.skip(4);
	            break;

	        /* istanbul ignore next */
	        default:
	            throw Error("invalid wire type " + wireType + " at offset " + this.pos);
	    }
	    return this;
	};

	Reader._configure = function(BufferReader_) {
	    BufferReader = BufferReader_;
	    Reader.create = create();
	    BufferReader._configure();

	    var fn = util.Long ? "toLong" : /* istanbul ignore next */ "toNumber";
	    util.merge(Reader.prototype, {

	        int64: function read_int64() {
	            return readLongVarint.call(this)[fn](false);
	        },

	        uint64: function read_uint64() {
	            return readLongVarint.call(this)[fn](true);
	        },

	        sint64: function read_sint64() {
	            return readLongVarint.call(this).zzDecode()[fn](false);
	        },

	        fixed64: function read_fixed64() {
	            return readFixed64.call(this)[fn](true);
	        },

	        sfixed64: function read_sfixed64() {
	            return readFixed64.call(this)[fn](false);
	        }

	    });
	};
	return reader;
}

var reader_buffer;
var hasRequiredReader_buffer;

function requireReader_buffer () {
	if (hasRequiredReader_buffer) return reader_buffer;
	hasRequiredReader_buffer = 1;
	reader_buffer = BufferReader;

	// extends Reader
	var Reader = requireReader();
	(BufferReader.prototype = Object.create(Reader.prototype)).constructor = BufferReader;

	var util = requireMinimal$1();

	/**
	 * Constructs a new buffer reader instance.
	 * @classdesc Wire format reader using node buffers.
	 * @extends Reader
	 * @constructor
	 * @param {Buffer} buffer Buffer to read from
	 */
	function BufferReader(buffer) {
	    Reader.call(this, buffer);

	    /**
	     * Read buffer.
	     * @name BufferReader#buf
	     * @type {Buffer}
	     */
	}

	BufferReader._configure = function () {
	    /* istanbul ignore else */
	    if (util.Buffer)
	        BufferReader.prototype._slice = util.Buffer.prototype.slice;
	};


	/**
	 * @override
	 */
	BufferReader.prototype.string = function read_string_buffer() {
	    var len = this.uint32(); // modifies pos
	    return this.buf.utf8Slice
	        ? this.buf.utf8Slice(this.pos, this.pos = Math.min(this.pos + len, this.len))
	        : this.buf.toString("utf-8", this.pos, this.pos = Math.min(this.pos + len, this.len));
	};

	/**
	 * Reads a sequence of bytes preceeded by its length as a varint.
	 * @name BufferReader#bytes
	 * @function
	 * @returns {Buffer} Value read
	 */

	BufferReader._configure();
	return reader_buffer;
}

var rpc = {};

var service;
var hasRequiredService;

function requireService () {
	if (hasRequiredService) return service;
	hasRequiredService = 1;
	service = Service;

	var util = requireMinimal$1();

	// Extends EventEmitter
	(Service.prototype = Object.create(util.EventEmitter.prototype)).constructor = Service;

	/**
	 * A service method callback as used by {@link rpc.ServiceMethod|ServiceMethod}.
	 *
	 * Differs from {@link RPCImplCallback} in that it is an actual callback of a service method which may not return `response = null`.
	 * @typedef rpc.ServiceMethodCallback
	 * @template TRes extends Message<TRes>
	 * @type {function}
	 * @param {Error|null} error Error, if any
	 * @param {TRes} [response] Response message
	 * @returns {undefined}
	 */

	/**
	 * A service method part of a {@link rpc.Service} as created by {@link Service.create}.
	 * @typedef rpc.ServiceMethod
	 * @template TReq extends Message<TReq>
	 * @template TRes extends Message<TRes>
	 * @type {function}
	 * @param {TReq|Properties<TReq>} request Request message or plain object
	 * @param {rpc.ServiceMethodCallback<TRes>} [callback] Node-style callback called with the error, if any, and the response message
	 * @returns {Promise<Message<TRes>>} Promise if `callback` has been omitted, otherwise `undefined`
	 */

	/**
	 * Constructs a new RPC service instance.
	 * @classdesc An RPC service as returned by {@link Service#create}.
	 * @exports rpc.Service
	 * @extends util.EventEmitter
	 * @constructor
	 * @param {RPCImpl} rpcImpl RPC implementation
	 * @param {boolean} [requestDelimited=false] Whether requests are length-delimited
	 * @param {boolean} [responseDelimited=false] Whether responses are length-delimited
	 */
	function Service(rpcImpl, requestDelimited, responseDelimited) {

	    if (typeof rpcImpl !== "function")
	        throw TypeError("rpcImpl must be a function");

	    util.EventEmitter.call(this);

	    /**
	     * RPC implementation. Becomes `null` once the service is ended.
	     * @type {RPCImpl|null}
	     */
	    this.rpcImpl = rpcImpl;

	    /**
	     * Whether requests are length-delimited.
	     * @type {boolean}
	     */
	    this.requestDelimited = Boolean(requestDelimited);

	    /**
	     * Whether responses are length-delimited.
	     * @type {boolean}
	     */
	    this.responseDelimited = Boolean(responseDelimited);
	}

	/**
	 * Calls a service method through {@link rpc.Service#rpcImpl|rpcImpl}.
	 * @param {Method|rpc.ServiceMethod<TReq,TRes>} method Reflected or static method
	 * @param {Constructor<TReq>} requestCtor Request constructor
	 * @param {Constructor<TRes>} responseCtor Response constructor
	 * @param {TReq|Properties<TReq>} request Request message or plain object
	 * @param {rpc.ServiceMethodCallback<TRes>} callback Service callback
	 * @returns {undefined}
	 * @template TReq extends Message<TReq>
	 * @template TRes extends Message<TRes>
	 */
	Service.prototype.rpcCall = function rpcCall(method, requestCtor, responseCtor, request, callback) {

	    if (!request)
	        throw TypeError("request must be specified");

	    var self = this;
	    if (!callback)
	        return util.asPromise(rpcCall, self, method, requestCtor, responseCtor, request);

	    if (!self.rpcImpl) {
	        setTimeout(function() { callback(Error("already ended")); }, 0);
	        return undefined;
	    }

	    try {
	        return self.rpcImpl(
	            method,
	            requestCtor[self.requestDelimited ? "encodeDelimited" : "encode"](request).finish(),
	            function rpcCallback(err, response) {

	                if (err) {
	                    self.emit("error", err, method);
	                    return callback(err);
	                }

	                if (response === null) {
	                    self.end(/* endedByRPC */ true);
	                    return undefined;
	                }

	                if (!(response instanceof responseCtor)) {
	                    try {
	                        response = responseCtor[self.responseDelimited ? "decodeDelimited" : "decode"](response);
	                    } catch (err) {
	                        self.emit("error", err, method);
	                        return callback(err);
	                    }
	                }

	                self.emit("data", response, method);
	                return callback(null, response);
	            }
	        );
	    } catch (err) {
	        self.emit("error", err, method);
	        setTimeout(function() { callback(err); }, 0);
	        return undefined;
	    }
	};

	/**
	 * Ends this service and emits the `end` event.
	 * @param {boolean} [endedByRPC=false] Whether the service has been ended by the RPC implementation.
	 * @returns {rpc.Service} `this`
	 */
	Service.prototype.end = function end(endedByRPC) {
	    if (this.rpcImpl) {
	        if (!endedByRPC) // signal end to rpcImpl
	            this.rpcImpl(null, null, null);
	        this.rpcImpl = null;
	        this.emit("end").off();
	    }
	    return this;
	};
	return service;
}

var hasRequiredRpc;

function requireRpc () {
	if (hasRequiredRpc) return rpc;
	hasRequiredRpc = 1;
	(function (exports) {

		/**
		 * Streaming RPC helpers.
		 * @namespace
		 */
		var rpc = exports;

		/**
		 * RPC implementation passed to {@link Service#create} performing a service request on network level, i.e. by utilizing http requests or websockets.
		 * @typedef RPCImpl
		 * @type {function}
		 * @param {Method|rpc.ServiceMethod<Message<{}>,Message<{}>>} method Reflected or static method being called
		 * @param {Uint8Array} requestData Request data
		 * @param {RPCImplCallback} callback Callback function
		 * @returns {undefined}
		 * @example
		 * function rpcImpl(method, requestData, callback) {
		 *     if (protobuf.util.lcFirst(method.name) !== "myMethod") // compatible with static code
		 *         throw Error("no such method");
		 *     asynchronouslyObtainAResponse(requestData, function(err, responseData) {
		 *         callback(err, responseData);
		 *     });
		 * }
		 */

		/**
		 * Node-style callback as used by {@link RPCImpl}.
		 * @typedef RPCImplCallback
		 * @type {function}
		 * @param {Error|null} error Error, if any, otherwise `null`
		 * @param {Uint8Array|null} [response] Response data or `null` to signal end of stream, if there hasn't been an error
		 * @returns {undefined}
		 */

		rpc.Service = requireService();
} (rpc));
	return rpc;
}

var roots;
var hasRequiredRoots;

function requireRoots () {
	if (hasRequiredRoots) return roots;
	hasRequiredRoots = 1;
	roots = {};

	/**
	 * Named roots.
	 * This is where pbjs stores generated structures (the option `-r, --root` specifies a name).
	 * Can also be used manually to make roots available accross modules.
	 * @name roots
	 * @type {Object.<string,Root>}
	 * @example
	 * // pbjs -r myroot -o compiled.js ...
	 *
	 * // in another module:
	 * require("./compiled.js");
	 *
	 * // in any subsequent module:
	 * var root = protobuf.roots["myroot"];
	 */
	return roots;
}

var hasRequiredIndexMinimal;

function requireIndexMinimal () {
	if (hasRequiredIndexMinimal) return indexMinimal;
	hasRequiredIndexMinimal = 1;
	(function (exports) {
		var protobuf = exports;

		/**
		 * Build type, one of `"full"`, `"light"` or `"minimal"`.
		 * @name build
		 * @type {string}
		 * @const
		 */
		protobuf.build = "minimal";

		// Serialization
		protobuf.Writer       = requireWriter();
		protobuf.BufferWriter = requireWriter_buffer();
		protobuf.Reader       = requireReader();
		protobuf.BufferReader = requireReader_buffer();

		// Utility
		protobuf.util         = requireMinimal$1();
		protobuf.rpc          = requireRpc();
		protobuf.roots        = requireRoots();
		protobuf.configure    = configure;

		/* istanbul ignore next */
		/**
		 * Reconfigures the library according to the environment.
		 * @returns {undefined}
		 */
		function configure() {
		    protobuf.util._configure();
		    protobuf.Writer._configure(protobuf.BufferWriter);
		    protobuf.Reader._configure(protobuf.BufferReader);
		}

		// Set up buffer utility according to the environment
		configure();
} (indexMinimal));
	return indexMinimal;
}

var hasRequiredMinimal;

function requireMinimal () {
	if (hasRequiredMinimal) return minimalExports$1;
	hasRequiredMinimal = 1;
	(function (module) {
		module.exports = requireIndexMinimal();
} (minimal$1));
	return minimalExports$1;
}

var minimalExports = requireMinimal();
var _m0 = /*@__PURE__*/getDefaultExportFromCjs(minimalExports);

/* eslint-disable */
function createBasePBAnimator() {
    return { states: [] };
}
/**
 * @public
 */
/**
 * @internal
 */
const PBAnimator = {
    encode(message, writer = _m0.Writer.create()) {
        for (const v of message.states) {
            PBAnimationState.encode(v, writer.uint32(10).fork()).ldelim();
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBAnimator();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.states.push(PBAnimationState.decode(reader, reader.uint32()));
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};
function createBasePBAnimationState() {
    return {
        name: "",
        clip: "",
        playing: undefined,
        weight: undefined,
        speed: undefined,
        loop: undefined,
        shouldReset: undefined,
    };
}
/**
 * @public
 */
/**
 * @internal
 */
const PBAnimationState = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.name !== "") {
            writer.uint32(10).string(message.name);
        }
        if (message.clip !== "") {
            writer.uint32(18).string(message.clip);
        }
        if (message.playing !== undefined) {
            writer.uint32(24).bool(message.playing);
        }
        if (message.weight !== undefined) {
            writer.uint32(37).float(message.weight);
        }
        if (message.speed !== undefined) {
            writer.uint32(45).float(message.speed);
        }
        if (message.loop !== undefined) {
            writer.uint32(48).bool(message.loop);
        }
        if (message.shouldReset !== undefined) {
            writer.uint32(56).bool(message.shouldReset);
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBAnimationState();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.name = reader.string();
                    break;
                case 2:
                    message.clip = reader.string();
                    break;
                case 3:
                    message.playing = reader.bool();
                    break;
                case 4:
                    message.weight = reader.float();
                    break;
                case 5:
                    message.speed = reader.float();
                    break;
                case 6:
                    message.loop = reader.bool();
                    break;
                case 7:
                    message.shouldReset = reader.bool();
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};

/**
 * @internal
 */
const AnimatorSchema = {
    COMPONENT_ID: 1042,
    serialize(value, builder) {
        const writer = PBAnimator.encode(value);
        const buffer = new Uint8Array(writer.finish(), 0, writer.len);
        builder.writeBuffer(buffer, false);
    },
    deserialize(reader) {
        return PBAnimator.decode(reader.buffer(), reader.remainingBytes());
    },
    create() {
        // TODO: this is a hack.
        return PBAnimator.decode(new Uint8Array());
    },
    jsonSchema: {
        type: "object",
        properties: {},
        serializationType: "protocol-buffer",
        protocolBuffer: "PBAnimator"
    }
};

/* eslint-disable */
function createBasePBAudioSource() {
    return { playing: undefined, volume: undefined, loop: undefined, pitch: undefined, audioClipUrl: "" };
}
/**
 * @public
 */
/**
 * @internal
 */
const PBAudioSource = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.playing !== undefined) {
            writer.uint32(8).bool(message.playing);
        }
        if (message.volume !== undefined) {
            writer.uint32(21).float(message.volume);
        }
        if (message.loop !== undefined) {
            writer.uint32(24).bool(message.loop);
        }
        if (message.pitch !== undefined) {
            writer.uint32(37).float(message.pitch);
        }
        if (message.audioClipUrl !== "") {
            writer.uint32(42).string(message.audioClipUrl);
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBAudioSource();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.playing = reader.bool();
                    break;
                case 2:
                    message.volume = reader.float();
                    break;
                case 3:
                    message.loop = reader.bool();
                    break;
                case 4:
                    message.pitch = reader.float();
                    break;
                case 5:
                    message.audioClipUrl = reader.string();
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};

/**
 * @internal
 */
const AudioSourceSchema = {
    COMPONENT_ID: 1020,
    serialize(value, builder) {
        const writer = PBAudioSource.encode(value);
        const buffer = new Uint8Array(writer.finish(), 0, writer.len);
        builder.writeBuffer(buffer, false);
    },
    deserialize(reader) {
        return PBAudioSource.decode(reader.buffer(), reader.remainingBytes());
    },
    create() {
        // TODO: this is a hack.
        return PBAudioSource.decode(new Uint8Array());
    },
    jsonSchema: {
        type: "object",
        properties: {},
        serializationType: "protocol-buffer",
        protocolBuffer: "PBAudioSource"
    }
};

/* eslint-disable */
function createBasePBAudioStream() {
    return { playing: undefined, volume: undefined, url: "" };
}
/**
 * @public
 */
/**
 * @internal
 */
const PBAudioStream = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.playing !== undefined) {
            writer.uint32(8).bool(message.playing);
        }
        if (message.volume !== undefined) {
            writer.uint32(21).float(message.volume);
        }
        if (message.url !== "") {
            writer.uint32(26).string(message.url);
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBAudioStream();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.playing = reader.bool();
                    break;
                case 2:
                    message.volume = reader.float();
                    break;
                case 3:
                    message.url = reader.string();
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};

/**
 * @internal
 */
const AudioStreamSchema = {
    COMPONENT_ID: 1021,
    serialize(value, builder) {
        const writer = PBAudioStream.encode(value);
        const buffer = new Uint8Array(writer.finish(), 0, writer.len);
        builder.writeBuffer(buffer, false);
    },
    deserialize(reader) {
        return PBAudioStream.decode(reader.buffer(), reader.remainingBytes());
    },
    create() {
        // TODO: this is a hack.
        return PBAudioStream.decode(new Uint8Array());
    },
    jsonSchema: {
        type: "object",
        properties: {},
        serializationType: "protocol-buffer",
        protocolBuffer: "PBAudioStream"
    }
};

/* eslint-disable */
/**
 * @public
 */
var AvatarAnchorPointType;
(function (AvatarAnchorPointType) {
    AvatarAnchorPointType[AvatarAnchorPointType["AAPT_POSITION"] = 0] = "AAPT_POSITION";
    AvatarAnchorPointType[AvatarAnchorPointType["AAPT_NAME_TAG"] = 1] = "AAPT_NAME_TAG";
    AvatarAnchorPointType[AvatarAnchorPointType["AAPT_LEFT_HAND"] = 2] = "AAPT_LEFT_HAND";
    AvatarAnchorPointType[AvatarAnchorPointType["AAPT_RIGHT_HAND"] = 3] = "AAPT_RIGHT_HAND";
})(AvatarAnchorPointType || (AvatarAnchorPointType = {}));
function createBasePBAvatarAttach() {
    return { avatarId: undefined, anchorPointId: 0 };
}
/**
 * @public
 */
/**
 * @internal
 */
const PBAvatarAttach = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.avatarId !== undefined) {
            writer.uint32(10).string(message.avatarId);
        }
        if (message.anchorPointId !== 0) {
            writer.uint32(16).int32(message.anchorPointId);
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBAvatarAttach();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.avatarId = reader.string();
                    break;
                case 2:
                    message.anchorPointId = reader.int32();
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};

/**
 * @internal
 */
const AvatarAttachSchema = {
    COMPONENT_ID: 1073,
    serialize(value, builder) {
        const writer = PBAvatarAttach.encode(value);
        const buffer = new Uint8Array(writer.finish(), 0, writer.len);
        builder.writeBuffer(buffer, false);
    },
    deserialize(reader) {
        return PBAvatarAttach.decode(reader.buffer(), reader.remainingBytes());
    },
    create() {
        // TODO: this is a hack.
        return PBAvatarAttach.decode(new Uint8Array());
    },
    jsonSchema: {
        type: "object",
        properties: {},
        serializationType: "protocol-buffer",
        protocolBuffer: "PBAvatarAttach"
    }
};

/* eslint-disable */
function createBaseVector3() {
    return { x: 0, y: 0, z: 0 };
}
/**
 * @public
 */
/**
 * @internal
 */
const Vector3$1 = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.x !== 0) {
            writer.uint32(13).float(message.x);
        }
        if (message.y !== 0) {
            writer.uint32(21).float(message.y);
        }
        if (message.z !== 0) {
            writer.uint32(29).float(message.z);
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBaseVector3();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.x = reader.float();
                    break;
                case 2:
                    message.y = reader.float();
                    break;
                case 3:
                    message.z = reader.float();
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};

/* eslint-disable */
/**
 * @public
 */
var AvatarModifierType;
(function (AvatarModifierType) {
    AvatarModifierType[AvatarModifierType["AMT_HIDE_AVATARS"] = 0] = "AMT_HIDE_AVATARS";
    AvatarModifierType[AvatarModifierType["AMT_DISABLE_PASSPORTS"] = 1] = "AMT_DISABLE_PASSPORTS";
})(AvatarModifierType || (AvatarModifierType = {}));
function createBasePBAvatarModifierArea() {
    return { area: undefined, excludeIds: [], modifiers: [] };
}
/**
 * @public
 */
/**
 * @internal
 */
const PBAvatarModifierArea = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.area !== undefined) {
            Vector3$1.encode(message.area, writer.uint32(10).fork()).ldelim();
        }
        for (const v of message.excludeIds) {
            writer.uint32(18).string(v);
        }
        writer.uint32(26).fork();
        for (const v of message.modifiers) {
            writer.int32(v);
        }
        writer.ldelim();
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBAvatarModifierArea();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.area = Vector3$1.decode(reader, reader.uint32());
                    break;
                case 2:
                    message.excludeIds.push(reader.string());
                    break;
                case 3:
                    if ((tag & 7) === 2) {
                        const end2 = reader.uint32() + reader.pos;
                        while (reader.pos < end2) {
                            message.modifiers.push(reader.int32());
                        }
                    }
                    else {
                        message.modifiers.push(reader.int32());
                    }
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};

/**
 * @internal
 */
const AvatarModifierAreaSchema = {
    COMPONENT_ID: 1070,
    serialize(value, builder) {
        const writer = PBAvatarModifierArea.encode(value);
        const buffer = new Uint8Array(writer.finish(), 0, writer.len);
        builder.writeBuffer(buffer, false);
    },
    deserialize(reader) {
        return PBAvatarModifierArea.decode(reader.buffer(), reader.remainingBytes());
    },
    create() {
        // TODO: this is a hack.
        return PBAvatarModifierArea.decode(new Uint8Array());
    },
    jsonSchema: {
        type: "object",
        properties: {},
        serializationType: "protocol-buffer",
        protocolBuffer: "PBAvatarModifierArea"
    }
};

var long;
var hasRequiredLong;

function requireLong () {
	if (hasRequiredLong) return long;
	hasRequiredLong = 1;
	long = Long;

	/**
	 * wasm optimizations, to do native i64 multiplication and divide
	 */
	var wasm = null;

	try {
	  wasm = new WebAssembly.Instance(new WebAssembly.Module(new Uint8Array([
	    0, 97, 115, 109, 1, 0, 0, 0, 1, 13, 2, 96, 0, 1, 127, 96, 4, 127, 127, 127, 127, 1, 127, 3, 7, 6, 0, 1, 1, 1, 1, 1, 6, 6, 1, 127, 1, 65, 0, 11, 7, 50, 6, 3, 109, 117, 108, 0, 1, 5, 100, 105, 118, 95, 115, 0, 2, 5, 100, 105, 118, 95, 117, 0, 3, 5, 114, 101, 109, 95, 115, 0, 4, 5, 114, 101, 109, 95, 117, 0, 5, 8, 103, 101, 116, 95, 104, 105, 103, 104, 0, 0, 10, 191, 1, 6, 4, 0, 35, 0, 11, 36, 1, 1, 126, 32, 0, 173, 32, 1, 173, 66, 32, 134, 132, 32, 2, 173, 32, 3, 173, 66, 32, 134, 132, 126, 34, 4, 66, 32, 135, 167, 36, 0, 32, 4, 167, 11, 36, 1, 1, 126, 32, 0, 173, 32, 1, 173, 66, 32, 134, 132, 32, 2, 173, 32, 3, 173, 66, 32, 134, 132, 127, 34, 4, 66, 32, 135, 167, 36, 0, 32, 4, 167, 11, 36, 1, 1, 126, 32, 0, 173, 32, 1, 173, 66, 32, 134, 132, 32, 2, 173, 32, 3, 173, 66, 32, 134, 132, 128, 34, 4, 66, 32, 135, 167, 36, 0, 32, 4, 167, 11, 36, 1, 1, 126, 32, 0, 173, 32, 1, 173, 66, 32, 134, 132, 32, 2, 173, 32, 3, 173, 66, 32, 134, 132, 129, 34, 4, 66, 32, 135, 167, 36, 0, 32, 4, 167, 11, 36, 1, 1, 126, 32, 0, 173, 32, 1, 173, 66, 32, 134, 132, 32, 2, 173, 32, 3, 173, 66, 32, 134, 132, 130, 34, 4, 66, 32, 135, 167, 36, 0, 32, 4, 167, 11
	  ])), {}).exports;
	} catch (e) {
	  // no wasm support :(
	}

	/**
	 * Constructs a 64 bit two's-complement integer, given its low and high 32 bit values as *signed* integers.
	 *  See the from* functions below for more convenient ways of constructing Longs.
	 * @exports Long
	 * @class A Long class for representing a 64 bit two's-complement integer value.
	 * @param {number} low The low (signed) 32 bits of the long
	 * @param {number} high The high (signed) 32 bits of the long
	 * @param {boolean=} unsigned Whether unsigned or not, defaults to signed
	 * @constructor
	 */
	function Long(low, high, unsigned) {

	    /**
	     * The low 32 bits as a signed value.
	     * @type {number}
	     */
	    this.low = low | 0;

	    /**
	     * The high 32 bits as a signed value.
	     * @type {number}
	     */
	    this.high = high | 0;

	    /**
	     * Whether unsigned or not.
	     * @type {boolean}
	     */
	    this.unsigned = !!unsigned;
	}

	Object.defineProperty(Long.prototype, "__isLong__", { value: true });

	/**
	 * @function
	 * @param {*} obj Object
	 * @returns {boolean}
	 * @inner
	 */
	function isLong(obj) {
	    return (obj && obj["__isLong__"]) === true;
	}

	/**
	 * Tests if the specified object is a Long.
	 * @function
	 * @param {*} obj Object
	 * @returns {boolean}
	 */
	Long.isLong = isLong;

	/**
	 * A cache of the Long representations of small integer values.
	 * @type {!Object}
	 * @inner
	 */
	var INT_CACHE = {};

	/**
	 * A cache of the Long representations of small unsigned integer values.
	 * @type {!Object}
	 * @inner
	 */
	var UINT_CACHE = {};

	/**
	 * @param {number} value
	 * @param {boolean=} unsigned
	 * @returns {!Long}
	 * @inner
	 */
	function fromInt(value, unsigned) {
	    var obj, cachedObj, cache;
	    if (unsigned) {
	        value >>>= 0;
	        if (cache = (0 <= value && value < 256)) {
	            cachedObj = UINT_CACHE[value];
	            if (cachedObj)
	                return cachedObj;
	        }
	        obj = fromBits(value, (value | 0) < 0 ? -1 : 0, true);
	        if (cache)
	            UINT_CACHE[value] = obj;
	        return obj;
	    } else {
	        value |= 0;
	        if (cache = (-128 <= value && value < 128)) {
	            cachedObj = INT_CACHE[value];
	            if (cachedObj)
	                return cachedObj;
	        }
	        obj = fromBits(value, value < 0 ? -1 : 0, false);
	        if (cache)
	            INT_CACHE[value] = obj;
	        return obj;
	    }
	}

	/**
	 * Returns a Long representing the given 32 bit integer value.
	 * @function
	 * @param {number} value The 32 bit integer in question
	 * @param {boolean=} unsigned Whether unsigned or not, defaults to signed
	 * @returns {!Long} The corresponding Long value
	 */
	Long.fromInt = fromInt;

	/**
	 * @param {number} value
	 * @param {boolean=} unsigned
	 * @returns {!Long}
	 * @inner
	 */
	function fromNumber(value, unsigned) {
	    if (isNaN(value))
	        return unsigned ? UZERO : ZERO;
	    if (unsigned) {
	        if (value < 0)
	            return UZERO;
	        if (value >= TWO_PWR_64_DBL)
	            return MAX_UNSIGNED_VALUE;
	    } else {
	        if (value <= -TWO_PWR_63_DBL)
	            return MIN_VALUE;
	        if (value + 1 >= TWO_PWR_63_DBL)
	            return MAX_VALUE;
	    }
	    if (value < 0)
	        return fromNumber(-value, unsigned).neg();
	    return fromBits((value % TWO_PWR_32_DBL) | 0, (value / TWO_PWR_32_DBL) | 0, unsigned);
	}

	/**
	 * Returns a Long representing the given value, provided that it is a finite number. Otherwise, zero is returned.
	 * @function
	 * @param {number} value The number in question
	 * @param {boolean=} unsigned Whether unsigned or not, defaults to signed
	 * @returns {!Long} The corresponding Long value
	 */
	Long.fromNumber = fromNumber;

	/**
	 * @param {number} lowBits
	 * @param {number} highBits
	 * @param {boolean=} unsigned
	 * @returns {!Long}
	 * @inner
	 */
	function fromBits(lowBits, highBits, unsigned) {
	    return new Long(lowBits, highBits, unsigned);
	}

	/**
	 * Returns a Long representing the 64 bit integer that comes by concatenating the given low and high bits. Each is
	 *  assumed to use 32 bits.
	 * @function
	 * @param {number} lowBits The low 32 bits
	 * @param {number} highBits The high 32 bits
	 * @param {boolean=} unsigned Whether unsigned or not, defaults to signed
	 * @returns {!Long} The corresponding Long value
	 */
	Long.fromBits = fromBits;

	/**
	 * @function
	 * @param {number} base
	 * @param {number} exponent
	 * @returns {number}
	 * @inner
	 */
	var pow_dbl = Math.pow; // Used 4 times (4*8 to 15+4)

	/**
	 * @param {string} str
	 * @param {(boolean|number)=} unsigned
	 * @param {number=} radix
	 * @returns {!Long}
	 * @inner
	 */
	function fromString(str, unsigned, radix) {
	    if (str.length === 0)
	        throw Error('empty string');
	    if (str === "NaN" || str === "Infinity" || str === "+Infinity" || str === "-Infinity")
	        return ZERO;
	    if (typeof unsigned === 'number') {
	        // For goog.math.long compatibility
	        radix = unsigned,
	        unsigned = false;
	    } else {
	        unsigned = !! unsigned;
	    }
	    radix = radix || 10;
	    if (radix < 2 || 36 < radix)
	        throw RangeError('radix');

	    var p;
	    if ((p = str.indexOf('-')) > 0)
	        throw Error('interior hyphen');
	    else if (p === 0) {
	        return fromString(str.substring(1), unsigned, radix).neg();
	    }

	    // Do several (8) digits each time through the loop, so as to
	    // minimize the calls to the very expensive emulated div.
	    var radixToPower = fromNumber(pow_dbl(radix, 8));

	    var result = ZERO;
	    for (var i = 0; i < str.length; i += 8) {
	        var size = Math.min(8, str.length - i),
	            value = parseInt(str.substring(i, i + size), radix);
	        if (size < 8) {
	            var power = fromNumber(pow_dbl(radix, size));
	            result = result.mul(power).add(fromNumber(value));
	        } else {
	            result = result.mul(radixToPower);
	            result = result.add(fromNumber(value));
	        }
	    }
	    result.unsigned = unsigned;
	    return result;
	}

	/**
	 * Returns a Long representation of the given string, written using the specified radix.
	 * @function
	 * @param {string} str The textual representation of the Long
	 * @param {(boolean|number)=} unsigned Whether unsigned or not, defaults to signed
	 * @param {number=} radix The radix in which the text is written (2-36), defaults to 10
	 * @returns {!Long} The corresponding Long value
	 */
	Long.fromString = fromString;

	/**
	 * @function
	 * @param {!Long|number|string|!{low: number, high: number, unsigned: boolean}} val
	 * @param {boolean=} unsigned
	 * @returns {!Long}
	 * @inner
	 */
	function fromValue(val, unsigned) {
	    if (typeof val === 'number')
	        return fromNumber(val, unsigned);
	    if (typeof val === 'string')
	        return fromString(val, unsigned);
	    // Throws for non-objects, converts non-instanceof Long:
	    return fromBits(val.low, val.high, typeof unsigned === 'boolean' ? unsigned : val.unsigned);
	}

	/**
	 * Converts the specified value to a Long using the appropriate from* function for its type.
	 * @function
	 * @param {!Long|number|string|!{low: number, high: number, unsigned: boolean}} val Value
	 * @param {boolean=} unsigned Whether unsigned or not, defaults to signed
	 * @returns {!Long}
	 */
	Long.fromValue = fromValue;

	// NOTE: the compiler should inline these constant values below and then remove these variables, so there should be
	// no runtime penalty for these.

	/**
	 * @type {number}
	 * @const
	 * @inner
	 */
	var TWO_PWR_16_DBL = 1 << 16;

	/**
	 * @type {number}
	 * @const
	 * @inner
	 */
	var TWO_PWR_24_DBL = 1 << 24;

	/**
	 * @type {number}
	 * @const
	 * @inner
	 */
	var TWO_PWR_32_DBL = TWO_PWR_16_DBL * TWO_PWR_16_DBL;

	/**
	 * @type {number}
	 * @const
	 * @inner
	 */
	var TWO_PWR_64_DBL = TWO_PWR_32_DBL * TWO_PWR_32_DBL;

	/**
	 * @type {number}
	 * @const
	 * @inner
	 */
	var TWO_PWR_63_DBL = TWO_PWR_64_DBL / 2;

	/**
	 * @type {!Long}
	 * @const
	 * @inner
	 */
	var TWO_PWR_24 = fromInt(TWO_PWR_24_DBL);

	/**
	 * @type {!Long}
	 * @inner
	 */
	var ZERO = fromInt(0);

	/**
	 * Signed zero.
	 * @type {!Long}
	 */
	Long.ZERO = ZERO;

	/**
	 * @type {!Long}
	 * @inner
	 */
	var UZERO = fromInt(0, true);

	/**
	 * Unsigned zero.
	 * @type {!Long}
	 */
	Long.UZERO = UZERO;

	/**
	 * @type {!Long}
	 * @inner
	 */
	var ONE = fromInt(1);

	/**
	 * Signed one.
	 * @type {!Long}
	 */
	Long.ONE = ONE;

	/**
	 * @type {!Long}
	 * @inner
	 */
	var UONE = fromInt(1, true);

	/**
	 * Unsigned one.
	 * @type {!Long}
	 */
	Long.UONE = UONE;

	/**
	 * @type {!Long}
	 * @inner
	 */
	var NEG_ONE = fromInt(-1);

	/**
	 * Signed negative one.
	 * @type {!Long}
	 */
	Long.NEG_ONE = NEG_ONE;

	/**
	 * @type {!Long}
	 * @inner
	 */
	var MAX_VALUE = fromBits(0xFFFFFFFF|0, 0x7FFFFFFF|0, false);

	/**
	 * Maximum signed value.
	 * @type {!Long}
	 */
	Long.MAX_VALUE = MAX_VALUE;

	/**
	 * @type {!Long}
	 * @inner
	 */
	var MAX_UNSIGNED_VALUE = fromBits(0xFFFFFFFF|0, 0xFFFFFFFF|0, true);

	/**
	 * Maximum unsigned value.
	 * @type {!Long}
	 */
	Long.MAX_UNSIGNED_VALUE = MAX_UNSIGNED_VALUE;

	/**
	 * @type {!Long}
	 * @inner
	 */
	var MIN_VALUE = fromBits(0, 0x80000000|0, false);

	/**
	 * Minimum signed value.
	 * @type {!Long}
	 */
	Long.MIN_VALUE = MIN_VALUE;

	/**
	 * @alias Long.prototype
	 * @inner
	 */
	var LongPrototype = Long.prototype;

	/**
	 * Converts the Long to a 32 bit integer, assuming it is a 32 bit integer.
	 * @returns {number}
	 */
	LongPrototype.toInt = function toInt() {
	    return this.unsigned ? this.low >>> 0 : this.low;
	};

	/**
	 * Converts the Long to a the nearest floating-point representation of this value (double, 53 bit mantissa).
	 * @returns {number}
	 */
	LongPrototype.toNumber = function toNumber() {
	    if (this.unsigned)
	        return ((this.high >>> 0) * TWO_PWR_32_DBL) + (this.low >>> 0);
	    return this.high * TWO_PWR_32_DBL + (this.low >>> 0);
	};

	/**
	 * Converts the Long to a string written in the specified radix.
	 * @param {number=} radix Radix (2-36), defaults to 10
	 * @returns {string}
	 * @override
	 * @throws {RangeError} If `radix` is out of range
	 */
	LongPrototype.toString = function toString(radix) {
	    radix = radix || 10;
	    if (radix < 2 || 36 < radix)
	        throw RangeError('radix');
	    if (this.isZero())
	        return '0';
	    if (this.isNegative()) { // Unsigned Longs are never negative
	        if (this.eq(MIN_VALUE)) {
	            // We need to change the Long value before it can be negated, so we remove
	            // the bottom-most digit in this base and then recurse to do the rest.
	            var radixLong = fromNumber(radix),
	                div = this.div(radixLong),
	                rem1 = div.mul(radixLong).sub(this);
	            return div.toString(radix) + rem1.toInt().toString(radix);
	        } else
	            return '-' + this.neg().toString(radix);
	    }

	    // Do several (6) digits each time through the loop, so as to
	    // minimize the calls to the very expensive emulated div.
	    var radixToPower = fromNumber(pow_dbl(radix, 6), this.unsigned),
	        rem = this;
	    var result = '';
	    while (true) {
	        var remDiv = rem.div(radixToPower),
	            intval = rem.sub(remDiv.mul(radixToPower)).toInt() >>> 0,
	            digits = intval.toString(radix);
	        rem = remDiv;
	        if (rem.isZero())
	            return digits + result;
	        else {
	            while (digits.length < 6)
	                digits = '0' + digits;
	            result = '' + digits + result;
	        }
	    }
	};

	/**
	 * Gets the high 32 bits as a signed integer.
	 * @returns {number} Signed high bits
	 */
	LongPrototype.getHighBits = function getHighBits() {
	    return this.high;
	};

	/**
	 * Gets the high 32 bits as an unsigned integer.
	 * @returns {number} Unsigned high bits
	 */
	LongPrototype.getHighBitsUnsigned = function getHighBitsUnsigned() {
	    return this.high >>> 0;
	};

	/**
	 * Gets the low 32 bits as a signed integer.
	 * @returns {number} Signed low bits
	 */
	LongPrototype.getLowBits = function getLowBits() {
	    return this.low;
	};

	/**
	 * Gets the low 32 bits as an unsigned integer.
	 * @returns {number} Unsigned low bits
	 */
	LongPrototype.getLowBitsUnsigned = function getLowBitsUnsigned() {
	    return this.low >>> 0;
	};

	/**
	 * Gets the number of bits needed to represent the absolute value of this Long.
	 * @returns {number}
	 */
	LongPrototype.getNumBitsAbs = function getNumBitsAbs() {
	    if (this.isNegative()) // Unsigned Longs are never negative
	        return this.eq(MIN_VALUE) ? 64 : this.neg().getNumBitsAbs();
	    var val = this.high != 0 ? this.high : this.low;
	    for (var bit = 31; bit > 0; bit--)
	        if ((val & (1 << bit)) != 0)
	            break;
	    return this.high != 0 ? bit + 33 : bit + 1;
	};

	/**
	 * Tests if this Long's value equals zero.
	 * @returns {boolean}
	 */
	LongPrototype.isZero = function isZero() {
	    return this.high === 0 && this.low === 0;
	};

	/**
	 * Tests if this Long's value equals zero. This is an alias of {@link Long#isZero}.
	 * @returns {boolean}
	 */
	LongPrototype.eqz = LongPrototype.isZero;

	/**
	 * Tests if this Long's value is negative.
	 * @returns {boolean}
	 */
	LongPrototype.isNegative = function isNegative() {
	    return !this.unsigned && this.high < 0;
	};

	/**
	 * Tests if this Long's value is positive.
	 * @returns {boolean}
	 */
	LongPrototype.isPositive = function isPositive() {
	    return this.unsigned || this.high >= 0;
	};

	/**
	 * Tests if this Long's value is odd.
	 * @returns {boolean}
	 */
	LongPrototype.isOdd = function isOdd() {
	    return (this.low & 1) === 1;
	};

	/**
	 * Tests if this Long's value is even.
	 * @returns {boolean}
	 */
	LongPrototype.isEven = function isEven() {
	    return (this.low & 1) === 0;
	};

	/**
	 * Tests if this Long's value equals the specified's.
	 * @param {!Long|number|string} other Other value
	 * @returns {boolean}
	 */
	LongPrototype.equals = function equals(other) {
	    if (!isLong(other))
	        other = fromValue(other);
	    if (this.unsigned !== other.unsigned && (this.high >>> 31) === 1 && (other.high >>> 31) === 1)
	        return false;
	    return this.high === other.high && this.low === other.low;
	};

	/**
	 * Tests if this Long's value equals the specified's. This is an alias of {@link Long#equals}.
	 * @function
	 * @param {!Long|number|string} other Other value
	 * @returns {boolean}
	 */
	LongPrototype.eq = LongPrototype.equals;

	/**
	 * Tests if this Long's value differs from the specified's.
	 * @param {!Long|number|string} other Other value
	 * @returns {boolean}
	 */
	LongPrototype.notEquals = function notEquals(other) {
	    return !this.eq(/* validates */ other);
	};

	/**
	 * Tests if this Long's value differs from the specified's. This is an alias of {@link Long#notEquals}.
	 * @function
	 * @param {!Long|number|string} other Other value
	 * @returns {boolean}
	 */
	LongPrototype.neq = LongPrototype.notEquals;

	/**
	 * Tests if this Long's value differs from the specified's. This is an alias of {@link Long#notEquals}.
	 * @function
	 * @param {!Long|number|string} other Other value
	 * @returns {boolean}
	 */
	LongPrototype.ne = LongPrototype.notEquals;

	/**
	 * Tests if this Long's value is less than the specified's.
	 * @param {!Long|number|string} other Other value
	 * @returns {boolean}
	 */
	LongPrototype.lessThan = function lessThan(other) {
	    return this.comp(/* validates */ other) < 0;
	};

	/**
	 * Tests if this Long's value is less than the specified's. This is an alias of {@link Long#lessThan}.
	 * @function
	 * @param {!Long|number|string} other Other value
	 * @returns {boolean}
	 */
	LongPrototype.lt = LongPrototype.lessThan;

	/**
	 * Tests if this Long's value is less than or equal the specified's.
	 * @param {!Long|number|string} other Other value
	 * @returns {boolean}
	 */
	LongPrototype.lessThanOrEqual = function lessThanOrEqual(other) {
	    return this.comp(/* validates */ other) <= 0;
	};

	/**
	 * Tests if this Long's value is less than or equal the specified's. This is an alias of {@link Long#lessThanOrEqual}.
	 * @function
	 * @param {!Long|number|string} other Other value
	 * @returns {boolean}
	 */
	LongPrototype.lte = LongPrototype.lessThanOrEqual;

	/**
	 * Tests if this Long's value is less than or equal the specified's. This is an alias of {@link Long#lessThanOrEqual}.
	 * @function
	 * @param {!Long|number|string} other Other value
	 * @returns {boolean}
	 */
	LongPrototype.le = LongPrototype.lessThanOrEqual;

	/**
	 * Tests if this Long's value is greater than the specified's.
	 * @param {!Long|number|string} other Other value
	 * @returns {boolean}
	 */
	LongPrototype.greaterThan = function greaterThan(other) {
	    return this.comp(/* validates */ other) > 0;
	};

	/**
	 * Tests if this Long's value is greater than the specified's. This is an alias of {@link Long#greaterThan}.
	 * @function
	 * @param {!Long|number|string} other Other value
	 * @returns {boolean}
	 */
	LongPrototype.gt = LongPrototype.greaterThan;

	/**
	 * Tests if this Long's value is greater than or equal the specified's.
	 * @param {!Long|number|string} other Other value
	 * @returns {boolean}
	 */
	LongPrototype.greaterThanOrEqual = function greaterThanOrEqual(other) {
	    return this.comp(/* validates */ other) >= 0;
	};

	/**
	 * Tests if this Long's value is greater than or equal the specified's. This is an alias of {@link Long#greaterThanOrEqual}.
	 * @function
	 * @param {!Long|number|string} other Other value
	 * @returns {boolean}
	 */
	LongPrototype.gte = LongPrototype.greaterThanOrEqual;

	/**
	 * Tests if this Long's value is greater than or equal the specified's. This is an alias of {@link Long#greaterThanOrEqual}.
	 * @function
	 * @param {!Long|number|string} other Other value
	 * @returns {boolean}
	 */
	LongPrototype.ge = LongPrototype.greaterThanOrEqual;

	/**
	 * Compares this Long's value with the specified's.
	 * @param {!Long|number|string} other Other value
	 * @returns {number} 0 if they are the same, 1 if the this is greater and -1
	 *  if the given one is greater
	 */
	LongPrototype.compare = function compare(other) {
	    if (!isLong(other))
	        other = fromValue(other);
	    if (this.eq(other))
	        return 0;
	    var thisNeg = this.isNegative(),
	        otherNeg = other.isNegative();
	    if (thisNeg && !otherNeg)
	        return -1;
	    if (!thisNeg && otherNeg)
	        return 1;
	    // At this point the sign bits are the same
	    if (!this.unsigned)
	        return this.sub(other).isNegative() ? -1 : 1;
	    // Both are positive if at least one is unsigned
	    return (other.high >>> 0) > (this.high >>> 0) || (other.high === this.high && (other.low >>> 0) > (this.low >>> 0)) ? -1 : 1;
	};

	/**
	 * Compares this Long's value with the specified's. This is an alias of {@link Long#compare}.
	 * @function
	 * @param {!Long|number|string} other Other value
	 * @returns {number} 0 if they are the same, 1 if the this is greater and -1
	 *  if the given one is greater
	 */
	LongPrototype.comp = LongPrototype.compare;

	/**
	 * Negates this Long's value.
	 * @returns {!Long} Negated Long
	 */
	LongPrototype.negate = function negate() {
	    if (!this.unsigned && this.eq(MIN_VALUE))
	        return MIN_VALUE;
	    return this.not().add(ONE);
	};

	/**
	 * Negates this Long's value. This is an alias of {@link Long#negate}.
	 * @function
	 * @returns {!Long} Negated Long
	 */
	LongPrototype.neg = LongPrototype.negate;

	/**
	 * Returns the sum of this and the specified Long.
	 * @param {!Long|number|string} addend Addend
	 * @returns {!Long} Sum
	 */
	LongPrototype.add = function add(addend) {
	    if (!isLong(addend))
	        addend = fromValue(addend);

	    // Divide each number into 4 chunks of 16 bits, and then sum the chunks.

	    var a48 = this.high >>> 16;
	    var a32 = this.high & 0xFFFF;
	    var a16 = this.low >>> 16;
	    var a00 = this.low & 0xFFFF;

	    var b48 = addend.high >>> 16;
	    var b32 = addend.high & 0xFFFF;
	    var b16 = addend.low >>> 16;
	    var b00 = addend.low & 0xFFFF;

	    var c48 = 0, c32 = 0, c16 = 0, c00 = 0;
	    c00 += a00 + b00;
	    c16 += c00 >>> 16;
	    c00 &= 0xFFFF;
	    c16 += a16 + b16;
	    c32 += c16 >>> 16;
	    c16 &= 0xFFFF;
	    c32 += a32 + b32;
	    c48 += c32 >>> 16;
	    c32 &= 0xFFFF;
	    c48 += a48 + b48;
	    c48 &= 0xFFFF;
	    return fromBits((c16 << 16) | c00, (c48 << 16) | c32, this.unsigned);
	};

	/**
	 * Returns the difference of this and the specified Long.
	 * @param {!Long|number|string} subtrahend Subtrahend
	 * @returns {!Long} Difference
	 */
	LongPrototype.subtract = function subtract(subtrahend) {
	    if (!isLong(subtrahend))
	        subtrahend = fromValue(subtrahend);
	    return this.add(subtrahend.neg());
	};

	/**
	 * Returns the difference of this and the specified Long. This is an alias of {@link Long#subtract}.
	 * @function
	 * @param {!Long|number|string} subtrahend Subtrahend
	 * @returns {!Long} Difference
	 */
	LongPrototype.sub = LongPrototype.subtract;

	/**
	 * Returns the product of this and the specified Long.
	 * @param {!Long|number|string} multiplier Multiplier
	 * @returns {!Long} Product
	 */
	LongPrototype.multiply = function multiply(multiplier) {
	    if (this.isZero())
	        return ZERO;
	    if (!isLong(multiplier))
	        multiplier = fromValue(multiplier);

	    // use wasm support if present
	    if (wasm) {
	        var low = wasm.mul(this.low,
	                           this.high,
	                           multiplier.low,
	                           multiplier.high);
	        return fromBits(low, wasm.get_high(), this.unsigned);
	    }

	    if (multiplier.isZero())
	        return ZERO;
	    if (this.eq(MIN_VALUE))
	        return multiplier.isOdd() ? MIN_VALUE : ZERO;
	    if (multiplier.eq(MIN_VALUE))
	        return this.isOdd() ? MIN_VALUE : ZERO;

	    if (this.isNegative()) {
	        if (multiplier.isNegative())
	            return this.neg().mul(multiplier.neg());
	        else
	            return this.neg().mul(multiplier).neg();
	    } else if (multiplier.isNegative())
	        return this.mul(multiplier.neg()).neg();

	    // If both longs are small, use float multiplication
	    if (this.lt(TWO_PWR_24) && multiplier.lt(TWO_PWR_24))
	        return fromNumber(this.toNumber() * multiplier.toNumber(), this.unsigned);

	    // Divide each long into 4 chunks of 16 bits, and then add up 4x4 products.
	    // We can skip products that would overflow.

	    var a48 = this.high >>> 16;
	    var a32 = this.high & 0xFFFF;
	    var a16 = this.low >>> 16;
	    var a00 = this.low & 0xFFFF;

	    var b48 = multiplier.high >>> 16;
	    var b32 = multiplier.high & 0xFFFF;
	    var b16 = multiplier.low >>> 16;
	    var b00 = multiplier.low & 0xFFFF;

	    var c48 = 0, c32 = 0, c16 = 0, c00 = 0;
	    c00 += a00 * b00;
	    c16 += c00 >>> 16;
	    c00 &= 0xFFFF;
	    c16 += a16 * b00;
	    c32 += c16 >>> 16;
	    c16 &= 0xFFFF;
	    c16 += a00 * b16;
	    c32 += c16 >>> 16;
	    c16 &= 0xFFFF;
	    c32 += a32 * b00;
	    c48 += c32 >>> 16;
	    c32 &= 0xFFFF;
	    c32 += a16 * b16;
	    c48 += c32 >>> 16;
	    c32 &= 0xFFFF;
	    c32 += a00 * b32;
	    c48 += c32 >>> 16;
	    c32 &= 0xFFFF;
	    c48 += a48 * b00 + a32 * b16 + a16 * b32 + a00 * b48;
	    c48 &= 0xFFFF;
	    return fromBits((c16 << 16) | c00, (c48 << 16) | c32, this.unsigned);
	};

	/**
	 * Returns the product of this and the specified Long. This is an alias of {@link Long#multiply}.
	 * @function
	 * @param {!Long|number|string} multiplier Multiplier
	 * @returns {!Long} Product
	 */
	LongPrototype.mul = LongPrototype.multiply;

	/**
	 * Returns this Long divided by the specified. The result is signed if this Long is signed or
	 *  unsigned if this Long is unsigned.
	 * @param {!Long|number|string} divisor Divisor
	 * @returns {!Long} Quotient
	 */
	LongPrototype.divide = function divide(divisor) {
	    if (!isLong(divisor))
	        divisor = fromValue(divisor);
	    if (divisor.isZero())
	        throw Error('division by zero');

	    // use wasm support if present
	    if (wasm) {
	        // guard against signed division overflow: the largest
	        // negative number / -1 would be 1 larger than the largest
	        // positive number, due to two's complement.
	        if (!this.unsigned &&
	            this.high === -0x80000000 &&
	            divisor.low === -1 && divisor.high === -1) {
	            // be consistent with non-wasm code path
	            return this;
	        }
	        var low = (this.unsigned ? wasm.div_u : wasm.div_s)(
	            this.low,
	            this.high,
	            divisor.low,
	            divisor.high
	        );
	        return fromBits(low, wasm.get_high(), this.unsigned);
	    }

	    if (this.isZero())
	        return this.unsigned ? UZERO : ZERO;
	    var approx, rem, res;
	    if (!this.unsigned) {
	        // This section is only relevant for signed longs and is derived from the
	        // closure library as a whole.
	        if (this.eq(MIN_VALUE)) {
	            if (divisor.eq(ONE) || divisor.eq(NEG_ONE))
	                return MIN_VALUE;  // recall that -MIN_VALUE == MIN_VALUE
	            else if (divisor.eq(MIN_VALUE))
	                return ONE;
	            else {
	                // At this point, we have |other| >= 2, so |this/other| < |MIN_VALUE|.
	                var halfThis = this.shr(1);
	                approx = halfThis.div(divisor).shl(1);
	                if (approx.eq(ZERO)) {
	                    return divisor.isNegative() ? ONE : NEG_ONE;
	                } else {
	                    rem = this.sub(divisor.mul(approx));
	                    res = approx.add(rem.div(divisor));
	                    return res;
	                }
	            }
	        } else if (divisor.eq(MIN_VALUE))
	            return this.unsigned ? UZERO : ZERO;
	        if (this.isNegative()) {
	            if (divisor.isNegative())
	                return this.neg().div(divisor.neg());
	            return this.neg().div(divisor).neg();
	        } else if (divisor.isNegative())
	            return this.div(divisor.neg()).neg();
	        res = ZERO;
	    } else {
	        // The algorithm below has not been made for unsigned longs. It's therefore
	        // required to take special care of the MSB prior to running it.
	        if (!divisor.unsigned)
	            divisor = divisor.toUnsigned();
	        if (divisor.gt(this))
	            return UZERO;
	        if (divisor.gt(this.shru(1))) // 15 >>> 1 = 7 ; with divisor = 8 ; true
	            return UONE;
	        res = UZERO;
	    }

	    // Repeat the following until the remainder is less than other:  find a
	    // floating-point that approximates remainder / other *from below*, add this
	    // into the result, and subtract it from the remainder.  It is critical that
	    // the approximate value is less than or equal to the real value so that the
	    // remainder never becomes negative.
	    rem = this;
	    while (rem.gte(divisor)) {
	        // Approximate the result of division. This may be a little greater or
	        // smaller than the actual value.
	        approx = Math.max(1, Math.floor(rem.toNumber() / divisor.toNumber()));

	        // We will tweak the approximate result by changing it in the 48-th digit or
	        // the smallest non-fractional digit, whichever is larger.
	        var log2 = Math.ceil(Math.log(approx) / Math.LN2),
	            delta = (log2 <= 48) ? 1 : pow_dbl(2, log2 - 48),

	        // Decrease the approximation until it is smaller than the remainder.  Note
	        // that if it is too large, the product overflows and is negative.
	            approxRes = fromNumber(approx),
	            approxRem = approxRes.mul(divisor);
	        while (approxRem.isNegative() || approxRem.gt(rem)) {
	            approx -= delta;
	            approxRes = fromNumber(approx, this.unsigned);
	            approxRem = approxRes.mul(divisor);
	        }

	        // We know the answer can't be zero... and actually, zero would cause
	        // infinite recursion since we would make no progress.
	        if (approxRes.isZero())
	            approxRes = ONE;

	        res = res.add(approxRes);
	        rem = rem.sub(approxRem);
	    }
	    return res;
	};

	/**
	 * Returns this Long divided by the specified. This is an alias of {@link Long#divide}.
	 * @function
	 * @param {!Long|number|string} divisor Divisor
	 * @returns {!Long} Quotient
	 */
	LongPrototype.div = LongPrototype.divide;

	/**
	 * Returns this Long modulo the specified.
	 * @param {!Long|number|string} divisor Divisor
	 * @returns {!Long} Remainder
	 */
	LongPrototype.modulo = function modulo(divisor) {
	    if (!isLong(divisor))
	        divisor = fromValue(divisor);

	    // use wasm support if present
	    if (wasm) {
	        var low = (this.unsigned ? wasm.rem_u : wasm.rem_s)(
	            this.low,
	            this.high,
	            divisor.low,
	            divisor.high
	        );
	        return fromBits(low, wasm.get_high(), this.unsigned);
	    }

	    return this.sub(this.div(divisor).mul(divisor));
	};

	/**
	 * Returns this Long modulo the specified. This is an alias of {@link Long#modulo}.
	 * @function
	 * @param {!Long|number|string} divisor Divisor
	 * @returns {!Long} Remainder
	 */
	LongPrototype.mod = LongPrototype.modulo;

	/**
	 * Returns this Long modulo the specified. This is an alias of {@link Long#modulo}.
	 * @function
	 * @param {!Long|number|string} divisor Divisor
	 * @returns {!Long} Remainder
	 */
	LongPrototype.rem = LongPrototype.modulo;

	/**
	 * Returns the bitwise NOT of this Long.
	 * @returns {!Long}
	 */
	LongPrototype.not = function not() {
	    return fromBits(~this.low, ~this.high, this.unsigned);
	};

	/**
	 * Returns the bitwise AND of this Long and the specified.
	 * @param {!Long|number|string} other Other Long
	 * @returns {!Long}
	 */
	LongPrototype.and = function and(other) {
	    if (!isLong(other))
	        other = fromValue(other);
	    return fromBits(this.low & other.low, this.high & other.high, this.unsigned);
	};

	/**
	 * Returns the bitwise OR of this Long and the specified.
	 * @param {!Long|number|string} other Other Long
	 * @returns {!Long}
	 */
	LongPrototype.or = function or(other) {
	    if (!isLong(other))
	        other = fromValue(other);
	    return fromBits(this.low | other.low, this.high | other.high, this.unsigned);
	};

	/**
	 * Returns the bitwise XOR of this Long and the given one.
	 * @param {!Long|number|string} other Other Long
	 * @returns {!Long}
	 */
	LongPrototype.xor = function xor(other) {
	    if (!isLong(other))
	        other = fromValue(other);
	    return fromBits(this.low ^ other.low, this.high ^ other.high, this.unsigned);
	};

	/**
	 * Returns this Long with bits shifted to the left by the given amount.
	 * @param {number|!Long} numBits Number of bits
	 * @returns {!Long} Shifted Long
	 */
	LongPrototype.shiftLeft = function shiftLeft(numBits) {
	    if (isLong(numBits))
	        numBits = numBits.toInt();
	    if ((numBits &= 63) === 0)
	        return this;
	    else if (numBits < 32)
	        return fromBits(this.low << numBits, (this.high << numBits) | (this.low >>> (32 - numBits)), this.unsigned);
	    else
	        return fromBits(0, this.low << (numBits - 32), this.unsigned);
	};

	/**
	 * Returns this Long with bits shifted to the left by the given amount. This is an alias of {@link Long#shiftLeft}.
	 * @function
	 * @param {number|!Long} numBits Number of bits
	 * @returns {!Long} Shifted Long
	 */
	LongPrototype.shl = LongPrototype.shiftLeft;

	/**
	 * Returns this Long with bits arithmetically shifted to the right by the given amount.
	 * @param {number|!Long} numBits Number of bits
	 * @returns {!Long} Shifted Long
	 */
	LongPrototype.shiftRight = function shiftRight(numBits) {
	    if (isLong(numBits))
	        numBits = numBits.toInt();
	    if ((numBits &= 63) === 0)
	        return this;
	    else if (numBits < 32)
	        return fromBits((this.low >>> numBits) | (this.high << (32 - numBits)), this.high >> numBits, this.unsigned);
	    else
	        return fromBits(this.high >> (numBits - 32), this.high >= 0 ? 0 : -1, this.unsigned);
	};

	/**
	 * Returns this Long with bits arithmetically shifted to the right by the given amount. This is an alias of {@link Long#shiftRight}.
	 * @function
	 * @param {number|!Long} numBits Number of bits
	 * @returns {!Long} Shifted Long
	 */
	LongPrototype.shr = LongPrototype.shiftRight;

	/**
	 * Returns this Long with bits logically shifted to the right by the given amount.
	 * @param {number|!Long} numBits Number of bits
	 * @returns {!Long} Shifted Long
	 */
	LongPrototype.shiftRightUnsigned = function shiftRightUnsigned(numBits) {
	    if (isLong(numBits))
	        numBits = numBits.toInt();
	    numBits &= 63;
	    if (numBits === 0)
	        return this;
	    else {
	        var high = this.high;
	        if (numBits < 32) {
	            var low = this.low;
	            return fromBits((low >>> numBits) | (high << (32 - numBits)), high >>> numBits, this.unsigned);
	        } else if (numBits === 32)
	            return fromBits(high, 0, this.unsigned);
	        else
	            return fromBits(high >>> (numBits - 32), 0, this.unsigned);
	    }
	};

	/**
	 * Returns this Long with bits logically shifted to the right by the given amount. This is an alias of {@link Long#shiftRightUnsigned}.
	 * @function
	 * @param {number|!Long} numBits Number of bits
	 * @returns {!Long} Shifted Long
	 */
	LongPrototype.shru = LongPrototype.shiftRightUnsigned;

	/**
	 * Returns this Long with bits logically shifted to the right by the given amount. This is an alias of {@link Long#shiftRightUnsigned}.
	 * @function
	 * @param {number|!Long} numBits Number of bits
	 * @returns {!Long} Shifted Long
	 */
	LongPrototype.shr_u = LongPrototype.shiftRightUnsigned;

	/**
	 * Converts this Long to signed.
	 * @returns {!Long} Signed long
	 */
	LongPrototype.toSigned = function toSigned() {
	    if (!this.unsigned)
	        return this;
	    return fromBits(this.low, this.high, false);
	};

	/**
	 * Converts this Long to unsigned.
	 * @returns {!Long} Unsigned long
	 */
	LongPrototype.toUnsigned = function toUnsigned() {
	    if (this.unsigned)
	        return this;
	    return fromBits(this.low, this.high, true);
	};

	/**
	 * Converts this Long to its byte representation.
	 * @param {boolean=} le Whether little or big endian, defaults to big endian
	 * @returns {!Array.<number>} Byte representation
	 */
	LongPrototype.toBytes = function toBytes(le) {
	    return le ? this.toBytesLE() : this.toBytesBE();
	};

	/**
	 * Converts this Long to its little endian byte representation.
	 * @returns {!Array.<number>} Little endian byte representation
	 */
	LongPrototype.toBytesLE = function toBytesLE() {
	    var hi = this.high,
	        lo = this.low;
	    return [
	        lo        & 0xff,
	        lo >>>  8 & 0xff,
	        lo >>> 16 & 0xff,
	        lo >>> 24       ,
	        hi        & 0xff,
	        hi >>>  8 & 0xff,
	        hi >>> 16 & 0xff,
	        hi >>> 24
	    ];
	};

	/**
	 * Converts this Long to its big endian byte representation.
	 * @returns {!Array.<number>} Big endian byte representation
	 */
	LongPrototype.toBytesBE = function toBytesBE() {
	    var hi = this.high,
	        lo = this.low;
	    return [
	        hi >>> 24       ,
	        hi >>> 16 & 0xff,
	        hi >>>  8 & 0xff,
	        hi        & 0xff,
	        lo >>> 24       ,
	        lo >>> 16 & 0xff,
	        lo >>>  8 & 0xff,
	        lo        & 0xff
	    ];
	};

	/**
	 * Creates a Long from its byte representation.
	 * @param {!Array.<number>} bytes Byte representation
	 * @param {boolean=} unsigned Whether unsigned or not, defaults to signed
	 * @param {boolean=} le Whether little or big endian, defaults to big endian
	 * @returns {Long} The corresponding Long value
	 */
	Long.fromBytes = function fromBytes(bytes, unsigned, le) {
	    return le ? Long.fromBytesLE(bytes, unsigned) : Long.fromBytesBE(bytes, unsigned);
	};

	/**
	 * Creates a Long from its little endian byte representation.
	 * @param {!Array.<number>} bytes Little endian byte representation
	 * @param {boolean=} unsigned Whether unsigned or not, defaults to signed
	 * @returns {Long} The corresponding Long value
	 */
	Long.fromBytesLE = function fromBytesLE(bytes, unsigned) {
	    return new Long(
	        bytes[0]       |
	        bytes[1] <<  8 |
	        bytes[2] << 16 |
	        bytes[3] << 24,
	        bytes[4]       |
	        bytes[5] <<  8 |
	        bytes[6] << 16 |
	        bytes[7] << 24,
	        unsigned
	    );
	};

	/**
	 * Creates a Long from its big endian byte representation.
	 * @param {!Array.<number>} bytes Big endian byte representation
	 * @param {boolean=} unsigned Whether unsigned or not, defaults to signed
	 * @returns {Long} The corresponding Long value
	 */
	Long.fromBytesBE = function fromBytesBE(bytes, unsigned) {
	    return new Long(
	        bytes[4] << 24 |
	        bytes[5] << 16 |
	        bytes[6] <<  8 |
	        bytes[7],
	        bytes[0] << 24 |
	        bytes[1] << 16 |
	        bytes[2] <<  8 |
	        bytes[3],
	        unsigned
	    );
	};
	return long;
}

var longExports = requireLong();
var Long = /*@__PURE__*/getDefaultExportFromCjs(longExports);

/* eslint-disable */
function createBaseColor3() {
    return { r: 0, g: 0, b: 0 };
}
/**
 * @public
 */
/**
 * @internal
 */
const Color3 = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.r !== 0) {
            writer.uint32(13).float(message.r);
        }
        if (message.g !== 0) {
            writer.uint32(21).float(message.g);
        }
        if (message.b !== 0) {
            writer.uint32(29).float(message.b);
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBaseColor3();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.r = reader.float();
                    break;
                case 2:
                    message.g = reader.float();
                    break;
                case 3:
                    message.b = reader.float();
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};
function createBaseColor4() {
    return { r: 0, g: 0, b: 0, a: 0 };
}
/**
 * @public
 */
/**
 * @internal
 */
const Color4$1 = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.r !== 0) {
            writer.uint32(13).float(message.r);
        }
        if (message.g !== 0) {
            writer.uint32(21).float(message.g);
        }
        if (message.b !== 0) {
            writer.uint32(29).float(message.b);
        }
        if (message.a !== 0) {
            writer.uint32(37).float(message.a);
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBaseColor4();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.r = reader.float();
                    break;
                case 2:
                    message.g = reader.float();
                    break;
                case 3:
                    message.b = reader.float();
                    break;
                case 4:
                    message.a = reader.float();
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};

/* eslint-disable */
function createBasePBAvatarShape() {
    return {
        id: "",
        name: undefined,
        bodyShape: undefined,
        skinColor: undefined,
        hairColor: undefined,
        eyeColor: undefined,
        expressionTriggerId: undefined,
        expressionTriggerTimestamp: undefined,
        talking: undefined,
        wearables: [],
        emotes: [],
    };
}
/**
 * @public
 */
/**
 * @internal
 */
const PBAvatarShape = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.id !== "") {
            writer.uint32(10).string(message.id);
        }
        if (message.name !== undefined) {
            writer.uint32(18).string(message.name);
        }
        if (message.bodyShape !== undefined) {
            writer.uint32(26).string(message.bodyShape);
        }
        if (message.skinColor !== undefined) {
            Color3.encode(message.skinColor, writer.uint32(34).fork()).ldelim();
        }
        if (message.hairColor !== undefined) {
            Color3.encode(message.hairColor, writer.uint32(42).fork()).ldelim();
        }
        if (message.eyeColor !== undefined) {
            Color3.encode(message.eyeColor, writer.uint32(50).fork()).ldelim();
        }
        if (message.expressionTriggerId !== undefined) {
            writer.uint32(58).string(message.expressionTriggerId);
        }
        if (message.expressionTriggerTimestamp !== undefined) {
            writer.uint32(64).int64(message.expressionTriggerTimestamp);
        }
        if (message.talking !== undefined) {
            writer.uint32(72).bool(message.talking);
        }
        for (const v of message.wearables) {
            writer.uint32(82).string(v);
        }
        for (const v of message.emotes) {
            writer.uint32(90).string(v);
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBAvatarShape();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.id = reader.string();
                    break;
                case 2:
                    message.name = reader.string();
                    break;
                case 3:
                    message.bodyShape = reader.string();
                    break;
                case 4:
                    message.skinColor = Color3.decode(reader, reader.uint32());
                    break;
                case 5:
                    message.hairColor = Color3.decode(reader, reader.uint32());
                    break;
                case 6:
                    message.eyeColor = Color3.decode(reader, reader.uint32());
                    break;
                case 7:
                    message.expressionTriggerId = reader.string();
                    break;
                case 8:
                    message.expressionTriggerTimestamp = longToNumber$1(reader.int64());
                    break;
                case 9:
                    message.talking = reader.bool();
                    break;
                case 10:
                    message.wearables.push(reader.string());
                    break;
                case 11:
                    message.emotes.push(reader.string());
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};
var tsProtoGlobalThis$1 = (() => {
    if (typeof globalThis !== "undefined") {
        return globalThis;
    }
    if (typeof self !== "undefined") {
        return self;
    }
    if (typeof global !== "undefined") {
        return global;
    }
    throw "Unable to locate global object";
})();
function longToNumber$1(long) {
    if (long.gt(Number.MAX_SAFE_INTEGER)) {
        throw new tsProtoGlobalThis$1.Error("Value is larger than Number.MAX_SAFE_INTEGER");
    }
    return long.toNumber();
}
if (_m0.util.Long !== Long) {
    _m0.util.Long = Long;
    _m0.configure();
}

/**
 * @internal
 */
const AvatarShapeSchema = {
    COMPONENT_ID: 1080,
    serialize(value, builder) {
        const writer = PBAvatarShape.encode(value);
        const buffer = new Uint8Array(writer.finish(), 0, writer.len);
        builder.writeBuffer(buffer, false);
    },
    deserialize(reader) {
        return PBAvatarShape.decode(reader.buffer(), reader.remainingBytes());
    },
    create() {
        // TODO: this is a hack.
        return PBAvatarShape.decode(new Uint8Array());
    },
    jsonSchema: {
        type: "object",
        properties: {},
        serializationType: "protocol-buffer",
        protocolBuffer: "PBAvatarShape"
    }
};

/* eslint-disable */
/**
 * @public
 */
var BillboardMode;
(function (BillboardMode) {
    BillboardMode[BillboardMode["BM_NONE"] = 0] = "BM_NONE";
    BillboardMode[BillboardMode["BM_X"] = 1] = "BM_X";
    BillboardMode[BillboardMode["BM_Y"] = 2] = "BM_Y";
    BillboardMode[BillboardMode["BM_Z"] = 4] = "BM_Z";
    BillboardMode[BillboardMode["BM_ALL"] = 7] = "BM_ALL";
})(BillboardMode || (BillboardMode = {}));
function createBasePBBillboard() {
    return { billboardMode: undefined };
}
/**
 * @public
 */
/**
 * @internal
 */
const PBBillboard = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.billboardMode !== undefined) {
            writer.uint32(8).int32(message.billboardMode);
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBBillboard();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.billboardMode = reader.int32();
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};

/**
 * @internal
 */
const BillboardSchema = {
    COMPONENT_ID: 1090,
    serialize(value, builder) {
        const writer = PBBillboard.encode(value);
        const buffer = new Uint8Array(writer.finish(), 0, writer.len);
        builder.writeBuffer(buffer, false);
    },
    deserialize(reader) {
        return PBBillboard.decode(reader.buffer(), reader.remainingBytes());
    },
    create() {
        // TODO: this is a hack.
        return PBBillboard.decode(new Uint8Array());
    },
    jsonSchema: {
        type: "object",
        properties: {},
        serializationType: "protocol-buffer",
        protocolBuffer: "PBBillboard"
    }
};

/* eslint-disable */
function createBasePBCameraMode() {
    return { mode: 0 };
}
/**
 * @public
 */
/**
 * @internal
 */
const PBCameraMode = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.mode !== 0) {
            writer.uint32(8).int32(message.mode);
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBCameraMode();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.mode = reader.int32();
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};

/**
 * @internal
 */
const CameraModeSchema = {
    COMPONENT_ID: 1072,
    serialize(value, builder) {
        const writer = PBCameraMode.encode(value);
        const buffer = new Uint8Array(writer.finish(), 0, writer.len);
        builder.writeBuffer(buffer, false);
    },
    deserialize(reader) {
        return PBCameraMode.decode(reader.buffer(), reader.remainingBytes());
    },
    create() {
        // TODO: this is a hack.
        return PBCameraMode.decode(new Uint8Array());
    },
    jsonSchema: {
        type: "object",
        properties: {},
        serializationType: "protocol-buffer",
        protocolBuffer: "PBCameraMode"
    }
};

/* eslint-disable */
function createBasePBCameraModeArea() {
    return { area: undefined, mode: 0 };
}
/**
 * @public
 */
/**
 * @internal
 */
const PBCameraModeArea = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.area !== undefined) {
            Vector3$1.encode(message.area, writer.uint32(10).fork()).ldelim();
        }
        if (message.mode !== 0) {
            writer.uint32(16).int32(message.mode);
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBCameraModeArea();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.area = Vector3$1.decode(reader, reader.uint32());
                    break;
                case 2:
                    message.mode = reader.int32();
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};

/**
 * @internal
 */
const CameraModeAreaSchema = {
    COMPONENT_ID: 1071,
    serialize(value, builder) {
        const writer = PBCameraModeArea.encode(value);
        const buffer = new Uint8Array(writer.finish(), 0, writer.len);
        builder.writeBuffer(buffer, false);
    },
    deserialize(reader) {
        return PBCameraModeArea.decode(reader.buffer(), reader.remainingBytes());
    },
    create() {
        // TODO: this is a hack.
        return PBCameraModeArea.decode(new Uint8Array());
    },
    jsonSchema: {
        type: "object",
        properties: {},
        serializationType: "protocol-buffer",
        protocolBuffer: "PBCameraModeArea"
    }
};

/* eslint-disable */
function createBasePBGltfContainer() {
    return { src: "" };
}
/**
 * @public
 */
/**
 * @internal
 */
const PBGltfContainer = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.src !== "") {
            writer.uint32(10).string(message.src);
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBGltfContainer();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.src = reader.string();
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};

/**
 * @internal
 */
const GltfContainerSchema = {
    COMPONENT_ID: 1041,
    serialize(value, builder) {
        const writer = PBGltfContainer.encode(value);
        const buffer = new Uint8Array(writer.finish(), 0, writer.len);
        builder.writeBuffer(buffer, false);
    },
    deserialize(reader) {
        return PBGltfContainer.decode(reader.buffer(), reader.remainingBytes());
    },
    create() {
        // TODO: this is a hack.
        return PBGltfContainer.decode(new Uint8Array());
    },
    jsonSchema: {
        type: "object",
        properties: {},
        serializationType: "protocol-buffer",
        protocolBuffer: "PBGltfContainer"
    }
};

/* eslint-disable */
/**
 * @public
 */
var TextureWrapMode;
(function (TextureWrapMode) {
    TextureWrapMode[TextureWrapMode["TWM_REPEAT"] = 0] = "TWM_REPEAT";
    TextureWrapMode[TextureWrapMode["TWM_CLAMP"] = 1] = "TWM_CLAMP";
    TextureWrapMode[TextureWrapMode["TWM_MIRROR"] = 2] = "TWM_MIRROR";
    TextureWrapMode[TextureWrapMode["TWM_MIRROR_ONCE"] = 3] = "TWM_MIRROR_ONCE";
})(TextureWrapMode || (TextureWrapMode = {}));
/**
 * @public
 */
var TextureFilterMode;
(function (TextureFilterMode) {
    TextureFilterMode[TextureFilterMode["TFM_POINT"] = 0] = "TFM_POINT";
    TextureFilterMode[TextureFilterMode["TFM_BILINEAR"] = 1] = "TFM_BILINEAR";
    TextureFilterMode[TextureFilterMode["TFM_TRILINEAR"] = 2] = "TFM_TRILINEAR";
})(TextureFilterMode || (TextureFilterMode = {}));
function createBaseTexture() {
    return { src: "", wrapMode: undefined, filterMode: undefined };
}
/**
 * @public
 */
/**
 * @internal
 */
const Texture = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.src !== "") {
            writer.uint32(10).string(message.src);
        }
        if (message.wrapMode !== undefined) {
            writer.uint32(16).int32(message.wrapMode);
        }
        if (message.filterMode !== undefined) {
            writer.uint32(24).int32(message.filterMode);
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBaseTexture();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.src = reader.string();
                    break;
                case 2:
                    message.wrapMode = reader.int32();
                    break;
                case 3:
                    message.filterMode = reader.int32();
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};
function createBaseAvatarTexture() {
    return { userId: "", wrapMode: undefined, filterMode: undefined };
}
/**
 * @public
 */
/**
 * @internal
 */
const AvatarTexture = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.userId !== "") {
            writer.uint32(10).string(message.userId);
        }
        if (message.wrapMode !== undefined) {
            writer.uint32(16).int32(message.wrapMode);
        }
        if (message.filterMode !== undefined) {
            writer.uint32(24).int32(message.filterMode);
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBaseAvatarTexture();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.userId = reader.string();
                    break;
                case 2:
                    message.wrapMode = reader.int32();
                    break;
                case 3:
                    message.filterMode = reader.int32();
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};
function createBaseVideoTexture() {
    return { videoPlayerEntity: 0, wrapMode: undefined, filterMode: undefined };
}
/**
 * @public
 */
/**
 * @internal
 */
const VideoTexture = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.videoPlayerEntity !== 0) {
            writer.uint32(8).uint32(message.videoPlayerEntity);
        }
        if (message.wrapMode !== undefined) {
            writer.uint32(16).int32(message.wrapMode);
        }
        if (message.filterMode !== undefined) {
            writer.uint32(24).int32(message.filterMode);
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBaseVideoTexture();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.videoPlayerEntity = reader.uint32();
                    break;
                case 2:
                    message.wrapMode = reader.int32();
                    break;
                case 3:
                    message.filterMode = reader.int32();
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};
function createBaseTextureUnion() {
    return { tex: undefined };
}
/**
 * @public
 */
/**
 * @internal
 */
const TextureUnion = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.tex?.$case === "texture") {
            Texture.encode(message.tex.texture, writer.uint32(10).fork()).ldelim();
        }
        if (message.tex?.$case === "avatarTexture") {
            AvatarTexture.encode(message.tex.avatarTexture, writer.uint32(18).fork()).ldelim();
        }
        if (message.tex?.$case === "videoTexture") {
            VideoTexture.encode(message.tex.videoTexture, writer.uint32(26).fork()).ldelim();
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBaseTextureUnion();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.tex = { $case: "texture", texture: Texture.decode(reader, reader.uint32()) };
                    break;
                case 2:
                    message.tex = { $case: "avatarTexture", avatarTexture: AvatarTexture.decode(reader, reader.uint32()) };
                    break;
                case 3:
                    message.tex = { $case: "videoTexture", videoTexture: VideoTexture.decode(reader, reader.uint32()) };
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};

/* eslint-disable */
/**
 * @public
 */
var MaterialTransparencyMode;
(function (MaterialTransparencyMode) {
    MaterialTransparencyMode[MaterialTransparencyMode["MTM_OPAQUE"] = 0] = "MTM_OPAQUE";
    MaterialTransparencyMode[MaterialTransparencyMode["MTM_ALPHA_TEST"] = 1] = "MTM_ALPHA_TEST";
    MaterialTransparencyMode[MaterialTransparencyMode["MTM_ALPHA_BLEND"] = 2] = "MTM_ALPHA_BLEND";
    MaterialTransparencyMode[MaterialTransparencyMode["MTM_ALPHA_TEST_AND_ALPHA_BLEND"] = 3] = "MTM_ALPHA_TEST_AND_ALPHA_BLEND";
    MaterialTransparencyMode[MaterialTransparencyMode["MTM_AUTO"] = 4] = "MTM_AUTO";
})(MaterialTransparencyMode || (MaterialTransparencyMode = {}));
function createBasePBMaterial() {
    return { material: undefined };
}
/**
 * @public
 */
/**
 * @internal
 */
const PBMaterial = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.material?.$case === "unlit") {
            PBMaterial_UnlitMaterial.encode(message.material.unlit, writer.uint32(10).fork()).ldelim();
        }
        if (message.material?.$case === "pbr") {
            PBMaterial_PbrMaterial.encode(message.material.pbr, writer.uint32(18).fork()).ldelim();
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBMaterial();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.material = { $case: "unlit", unlit: PBMaterial_UnlitMaterial.decode(reader, reader.uint32()) };
                    break;
                case 2:
                    message.material = { $case: "pbr", pbr: PBMaterial_PbrMaterial.decode(reader, reader.uint32()) };
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};
function createBasePBMaterial_UnlitMaterial() {
    return { texture: undefined, alphaTest: undefined, castShadows: undefined, diffuseColor: undefined };
}
/**
 * @public
 */
/**
 * @internal
 */
const PBMaterial_UnlitMaterial = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.texture !== undefined) {
            TextureUnion.encode(message.texture, writer.uint32(10).fork()).ldelim();
        }
        if (message.alphaTest !== undefined) {
            writer.uint32(21).float(message.alphaTest);
        }
        if (message.castShadows !== undefined) {
            writer.uint32(24).bool(message.castShadows);
        }
        if (message.diffuseColor !== undefined) {
            Color4$1.encode(message.diffuseColor, writer.uint32(34).fork()).ldelim();
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBMaterial_UnlitMaterial();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.texture = TextureUnion.decode(reader, reader.uint32());
                    break;
                case 2:
                    message.alphaTest = reader.float();
                    break;
                case 3:
                    message.castShadows = reader.bool();
                    break;
                case 4:
                    message.diffuseColor = Color4$1.decode(reader, reader.uint32());
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};
function createBasePBMaterial_PbrMaterial() {
    return {
        texture: undefined,
        alphaTest: undefined,
        castShadows: undefined,
        alphaTexture: undefined,
        emissiveTexture: undefined,
        bumpTexture: undefined,
        albedoColor: undefined,
        emissiveColor: undefined,
        reflectivityColor: undefined,
        transparencyMode: undefined,
        metallic: undefined,
        roughness: undefined,
        glossiness: undefined,
        specularIntensity: undefined,
        emissiveIntensity: undefined,
        directIntensity: undefined,
    };
}
/**
 * @public
 */
/**
 * @internal
 */
const PBMaterial_PbrMaterial = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.texture !== undefined) {
            TextureUnion.encode(message.texture, writer.uint32(10).fork()).ldelim();
        }
        if (message.alphaTest !== undefined) {
            writer.uint32(21).float(message.alphaTest);
        }
        if (message.castShadows !== undefined) {
            writer.uint32(24).bool(message.castShadows);
        }
        if (message.alphaTexture !== undefined) {
            TextureUnion.encode(message.alphaTexture, writer.uint32(34).fork()).ldelim();
        }
        if (message.emissiveTexture !== undefined) {
            TextureUnion.encode(message.emissiveTexture, writer.uint32(42).fork()).ldelim();
        }
        if (message.bumpTexture !== undefined) {
            TextureUnion.encode(message.bumpTexture, writer.uint32(50).fork()).ldelim();
        }
        if (message.albedoColor !== undefined) {
            Color4$1.encode(message.albedoColor, writer.uint32(58).fork()).ldelim();
        }
        if (message.emissiveColor !== undefined) {
            Color3.encode(message.emissiveColor, writer.uint32(66).fork()).ldelim();
        }
        if (message.reflectivityColor !== undefined) {
            Color3.encode(message.reflectivityColor, writer.uint32(74).fork()).ldelim();
        }
        if (message.transparencyMode !== undefined) {
            writer.uint32(80).int32(message.transparencyMode);
        }
        if (message.metallic !== undefined) {
            writer.uint32(93).float(message.metallic);
        }
        if (message.roughness !== undefined) {
            writer.uint32(101).float(message.roughness);
        }
        if (message.glossiness !== undefined) {
            writer.uint32(109).float(message.glossiness);
        }
        if (message.specularIntensity !== undefined) {
            writer.uint32(117).float(message.specularIntensity);
        }
        if (message.emissiveIntensity !== undefined) {
            writer.uint32(125).float(message.emissiveIntensity);
        }
        if (message.directIntensity !== undefined) {
            writer.uint32(133).float(message.directIntensity);
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBMaterial_PbrMaterial();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.texture = TextureUnion.decode(reader, reader.uint32());
                    break;
                case 2:
                    message.alphaTest = reader.float();
                    break;
                case 3:
                    message.castShadows = reader.bool();
                    break;
                case 4:
                    message.alphaTexture = TextureUnion.decode(reader, reader.uint32());
                    break;
                case 5:
                    message.emissiveTexture = TextureUnion.decode(reader, reader.uint32());
                    break;
                case 6:
                    message.bumpTexture = TextureUnion.decode(reader, reader.uint32());
                    break;
                case 7:
                    message.albedoColor = Color4$1.decode(reader, reader.uint32());
                    break;
                case 8:
                    message.emissiveColor = Color3.decode(reader, reader.uint32());
                    break;
                case 9:
                    message.reflectivityColor = Color3.decode(reader, reader.uint32());
                    break;
                case 10:
                    message.transparencyMode = reader.int32();
                    break;
                case 11:
                    message.metallic = reader.float();
                    break;
                case 12:
                    message.roughness = reader.float();
                    break;
                case 13:
                    message.glossiness = reader.float();
                    break;
                case 14:
                    message.specularIntensity = reader.float();
                    break;
                case 15:
                    message.emissiveIntensity = reader.float();
                    break;
                case 16:
                    message.directIntensity = reader.float();
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};

/**
 * @internal
 */
const MaterialSchema = {
    COMPONENT_ID: 1017,
    serialize(value, builder) {
        const writer = PBMaterial.encode(value);
        const buffer = new Uint8Array(writer.finish(), 0, writer.len);
        builder.writeBuffer(buffer, false);
    },
    deserialize(reader) {
        return PBMaterial.decode(reader.buffer(), reader.remainingBytes());
    },
    create() {
        // TODO: this is a hack.
        return PBMaterial.decode(new Uint8Array());
    },
    jsonSchema: {
        type: "object",
        properties: {},
        serializationType: "protocol-buffer",
        protocolBuffer: "PBMaterial"
    }
};

/* eslint-disable */
/**
 * @public
 */
var ColliderLayer;
(function (ColliderLayer) {
    ColliderLayer[ColliderLayer["CL_NONE"] = 0] = "CL_NONE";
    ColliderLayer[ColliderLayer["CL_POINTER"] = 1] = "CL_POINTER";
    ColliderLayer[ColliderLayer["CL_PHYSICS"] = 2] = "CL_PHYSICS";
})(ColliderLayer || (ColliderLayer = {}));
function createBasePBMeshCollider() {
    return { collisionMask: undefined, mesh: undefined };
}
/**
 * @public
 */
/**
 * @internal
 */
const PBMeshCollider = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.collisionMask !== undefined) {
            writer.uint32(8).int32(message.collisionMask);
        }
        if (message.mesh?.$case === "box") {
            PBMeshCollider_BoxMesh.encode(message.mesh.box, writer.uint32(18).fork()).ldelim();
        }
        if (message.mesh?.$case === "sphere") {
            PBMeshCollider_SphereMesh.encode(message.mesh.sphere, writer.uint32(26).fork()).ldelim();
        }
        if (message.mesh?.$case === "cylinder") {
            PBMeshCollider_CylinderMesh.encode(message.mesh.cylinder, writer.uint32(34).fork()).ldelim();
        }
        if (message.mesh?.$case === "plane") {
            PBMeshCollider_PlaneMesh.encode(message.mesh.plane, writer.uint32(42).fork()).ldelim();
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBMeshCollider();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.collisionMask = reader.int32();
                    break;
                case 2:
                    message.mesh = { $case: "box", box: PBMeshCollider_BoxMesh.decode(reader, reader.uint32()) };
                    break;
                case 3:
                    message.mesh = { $case: "sphere", sphere: PBMeshCollider_SphereMesh.decode(reader, reader.uint32()) };
                    break;
                case 4:
                    message.mesh = { $case: "cylinder", cylinder: PBMeshCollider_CylinderMesh.decode(reader, reader.uint32()) };
                    break;
                case 5:
                    message.mesh = { $case: "plane", plane: PBMeshCollider_PlaneMesh.decode(reader, reader.uint32()) };
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};
function createBasePBMeshCollider_BoxMesh() {
    return {};
}
/**
 * @public
 */
/**
 * @internal
 */
const PBMeshCollider_BoxMesh = {
    encode(_, writer = _m0.Writer.create()) {
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBMeshCollider_BoxMesh();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};
function createBasePBMeshCollider_CylinderMesh() {
    return { radiusTop: undefined, radiusBottom: undefined };
}
/**
 * @public
 */
/**
 * @internal
 */
const PBMeshCollider_CylinderMesh = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.radiusTop !== undefined) {
            writer.uint32(13).float(message.radiusTop);
        }
        if (message.radiusBottom !== undefined) {
            writer.uint32(21).float(message.radiusBottom);
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBMeshCollider_CylinderMesh();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.radiusTop = reader.float();
                    break;
                case 2:
                    message.radiusBottom = reader.float();
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};
function createBasePBMeshCollider_PlaneMesh() {
    return {};
}
/**
 * @public
 */
/**
 * @internal
 */
const PBMeshCollider_PlaneMesh = {
    encode(_, writer = _m0.Writer.create()) {
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBMeshCollider_PlaneMesh();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};
function createBasePBMeshCollider_SphereMesh() {
    return {};
}
/**
 * @public
 */
/**
 * @internal
 */
const PBMeshCollider_SphereMesh = {
    encode(_, writer = _m0.Writer.create()) {
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBMeshCollider_SphereMesh();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};

/**
 * @internal
 */
const MeshColliderSchema = {
    COMPONENT_ID: 1019,
    serialize(value, builder) {
        const writer = PBMeshCollider.encode(value);
        const buffer = new Uint8Array(writer.finish(), 0, writer.len);
        builder.writeBuffer(buffer, false);
    },
    deserialize(reader) {
        return PBMeshCollider.decode(reader.buffer(), reader.remainingBytes());
    },
    create() {
        // TODO: this is a hack.
        return PBMeshCollider.decode(new Uint8Array());
    },
    jsonSchema: {
        type: "object",
        properties: {},
        serializationType: "protocol-buffer",
        protocolBuffer: "PBMeshCollider"
    }
};

/* eslint-disable */
function createBasePBMeshRenderer() {
    return { mesh: undefined };
}
/**
 * @public
 */
/**
 * @internal
 */
const PBMeshRenderer = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.mesh?.$case === "box") {
            PBMeshRenderer_BoxMesh.encode(message.mesh.box, writer.uint32(10).fork()).ldelim();
        }
        if (message.mesh?.$case === "sphere") {
            PBMeshRenderer_SphereMesh.encode(message.mesh.sphere, writer.uint32(18).fork()).ldelim();
        }
        if (message.mesh?.$case === "cylinder") {
            PBMeshRenderer_CylinderMesh.encode(message.mesh.cylinder, writer.uint32(26).fork()).ldelim();
        }
        if (message.mesh?.$case === "plane") {
            PBMeshRenderer_PlaneMesh.encode(message.mesh.plane, writer.uint32(34).fork()).ldelim();
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBMeshRenderer();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.mesh = { $case: "box", box: PBMeshRenderer_BoxMesh.decode(reader, reader.uint32()) };
                    break;
                case 2:
                    message.mesh = { $case: "sphere", sphere: PBMeshRenderer_SphereMesh.decode(reader, reader.uint32()) };
                    break;
                case 3:
                    message.mesh = { $case: "cylinder", cylinder: PBMeshRenderer_CylinderMesh.decode(reader, reader.uint32()) };
                    break;
                case 4:
                    message.mesh = { $case: "plane", plane: PBMeshRenderer_PlaneMesh.decode(reader, reader.uint32()) };
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};
function createBasePBMeshRenderer_BoxMesh() {
    return { uvs: [] };
}
/**
 * @public
 */
/**
 * @internal
 */
const PBMeshRenderer_BoxMesh = {
    encode(message, writer = _m0.Writer.create()) {
        writer.uint32(10).fork();
        for (const v of message.uvs) {
            writer.float(v);
        }
        writer.ldelim();
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBMeshRenderer_BoxMesh();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    if ((tag & 7) === 2) {
                        const end2 = reader.uint32() + reader.pos;
                        while (reader.pos < end2) {
                            message.uvs.push(reader.float());
                        }
                    }
                    else {
                        message.uvs.push(reader.float());
                    }
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};
function createBasePBMeshRenderer_CylinderMesh() {
    return { radiusTop: undefined, radiusBottom: undefined };
}
/**
 * @public
 */
/**
 * @internal
 */
const PBMeshRenderer_CylinderMesh = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.radiusTop !== undefined) {
            writer.uint32(13).float(message.radiusTop);
        }
        if (message.radiusBottom !== undefined) {
            writer.uint32(21).float(message.radiusBottom);
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBMeshRenderer_CylinderMesh();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.radiusTop = reader.float();
                    break;
                case 2:
                    message.radiusBottom = reader.float();
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};
function createBasePBMeshRenderer_PlaneMesh() {
    return { uvs: [] };
}
/**
 * @public
 */
/**
 * @internal
 */
const PBMeshRenderer_PlaneMesh = {
    encode(message, writer = _m0.Writer.create()) {
        writer.uint32(10).fork();
        for (const v of message.uvs) {
            writer.float(v);
        }
        writer.ldelim();
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBMeshRenderer_PlaneMesh();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    if ((tag & 7) === 2) {
                        const end2 = reader.uint32() + reader.pos;
                        while (reader.pos < end2) {
                            message.uvs.push(reader.float());
                        }
                    }
                    else {
                        message.uvs.push(reader.float());
                    }
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};
function createBasePBMeshRenderer_SphereMesh() {
    return {};
}
/**
 * @public
 */
/**
 * @internal
 */
const PBMeshRenderer_SphereMesh = {
    encode(_, writer = _m0.Writer.create()) {
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBMeshRenderer_SphereMesh();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};

/**
 * @internal
 */
const MeshRendererSchema = {
    COMPONENT_ID: 1018,
    serialize(value, builder) {
        const writer = PBMeshRenderer.encode(value);
        const buffer = new Uint8Array(writer.finish(), 0, writer.len);
        builder.writeBuffer(buffer, false);
    },
    deserialize(reader) {
        return PBMeshRenderer.decode(reader.buffer(), reader.remainingBytes());
    },
    create() {
        // TODO: this is a hack.
        return PBMeshRenderer.decode(new Uint8Array());
    },
    jsonSchema: {
        type: "object",
        properties: {},
        serializationType: "protocol-buffer",
        protocolBuffer: "PBMeshRenderer"
    }
};

/* eslint-disable */
/**
 * @public
 */
var NftFrameType;
(function (NftFrameType) {
    NftFrameType[NftFrameType["NFT_CLASSIC"] = 0] = "NFT_CLASSIC";
    NftFrameType[NftFrameType["NFT_BAROQUE_ORNAMENT"] = 1] = "NFT_BAROQUE_ORNAMENT";
    NftFrameType[NftFrameType["NFT_DIAMOND_ORNAMENT"] = 2] = "NFT_DIAMOND_ORNAMENT";
    NftFrameType[NftFrameType["NFT_MINIMAL_WIDE"] = 3] = "NFT_MINIMAL_WIDE";
    NftFrameType[NftFrameType["NFT_MINIMAL_GREY"] = 4] = "NFT_MINIMAL_GREY";
    NftFrameType[NftFrameType["NFT_BLOCKY"] = 5] = "NFT_BLOCKY";
    NftFrameType[NftFrameType["NFT_GOLD_EDGES"] = 6] = "NFT_GOLD_EDGES";
    NftFrameType[NftFrameType["NFT_GOLD_CARVED"] = 7] = "NFT_GOLD_CARVED";
    NftFrameType[NftFrameType["NFT_GOLD_WIDE"] = 8] = "NFT_GOLD_WIDE";
    NftFrameType[NftFrameType["NFT_GOLD_ROUNDED"] = 9] = "NFT_GOLD_ROUNDED";
    NftFrameType[NftFrameType["NFT_METAL_MEDIUM"] = 10] = "NFT_METAL_MEDIUM";
    NftFrameType[NftFrameType["NFT_METAL_WIDE"] = 11] = "NFT_METAL_WIDE";
    NftFrameType[NftFrameType["NFT_METAL_SLIM"] = 12] = "NFT_METAL_SLIM";
    NftFrameType[NftFrameType["NFT_METAL_ROUNDED"] = 13] = "NFT_METAL_ROUNDED";
    NftFrameType[NftFrameType["NFT_PINS"] = 14] = "NFT_PINS";
    NftFrameType[NftFrameType["NFT_MINIMAL_BLACK"] = 15] = "NFT_MINIMAL_BLACK";
    NftFrameType[NftFrameType["NFT_MINIMAL_WHITE"] = 16] = "NFT_MINIMAL_WHITE";
    NftFrameType[NftFrameType["NFT_TAPE"] = 17] = "NFT_TAPE";
    NftFrameType[NftFrameType["NFT_WOOD_SLIM"] = 18] = "NFT_WOOD_SLIM";
    NftFrameType[NftFrameType["NFT_WOOD_WIDE"] = 19] = "NFT_WOOD_WIDE";
    NftFrameType[NftFrameType["NFT_WOOD_TWIGS"] = 20] = "NFT_WOOD_TWIGS";
    NftFrameType[NftFrameType["NFT_CANVAS"] = 21] = "NFT_CANVAS";
    NftFrameType[NftFrameType["NFT_NONE"] = 22] = "NFT_NONE";
})(NftFrameType || (NftFrameType = {}));
function createBasePBNftShape() {
    return { src: "", style: undefined, color: undefined };
}
/**
 * @public
 */
/**
 * @internal
 */
const PBNftShape = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.src !== "") {
            writer.uint32(10).string(message.src);
        }
        if (message.style !== undefined) {
            writer.uint32(16).int32(message.style);
        }
        if (message.color !== undefined) {
            Color3.encode(message.color, writer.uint32(26).fork()).ldelim();
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBNftShape();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.src = reader.string();
                    break;
                case 2:
                    message.style = reader.int32();
                    break;
                case 3:
                    message.color = Color3.decode(reader, reader.uint32());
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};

/**
 * @internal
 */
const NftShapeSchema = {
    COMPONENT_ID: 1040,
    serialize(value, builder) {
        const writer = PBNftShape.encode(value);
        const buffer = new Uint8Array(writer.finish(), 0, writer.len);
        builder.writeBuffer(buffer, false);
    },
    deserialize(reader) {
        return PBNftShape.decode(reader.buffer(), reader.remainingBytes());
    },
    create() {
        // TODO: this is a hack.
        return PBNftShape.decode(new Uint8Array());
    },
    jsonSchema: {
        type: "object",
        properties: {},
        serializationType: "protocol-buffer",
        protocolBuffer: "PBNftShape"
    }
};

/* eslint-disable */
/**
 * @public
 */
var PointerEventType;
(function (PointerEventType) {
    PointerEventType[PointerEventType["PET_UP"] = 0] = "PET_UP";
    PointerEventType[PointerEventType["PET_DOWN"] = 1] = "PET_DOWN";
    PointerEventType[PointerEventType["PET_HOVER_ENTER"] = 2] = "PET_HOVER_ENTER";
    PointerEventType[PointerEventType["PET_HOVER_LEAVE"] = 3] = "PET_HOVER_LEAVE";
})(PointerEventType || (PointerEventType = {}));
function createBasePBPointerEvents() {
    return { pointerEvents: [] };
}
/**
 * @public
 */
/**
 * @internal
 */
const PBPointerEvents = {
    encode(message, writer = _m0.Writer.create()) {
        for (const v of message.pointerEvents) {
            PBPointerEvents_Entry.encode(v, writer.uint32(10).fork()).ldelim();
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBPointerEvents();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.pointerEvents.push(PBPointerEvents_Entry.decode(reader, reader.uint32()));
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};
function createBasePBPointerEvents_Info() {
    return { button: undefined, hoverText: undefined, maxDistance: undefined, showFeedback: undefined };
}
/**
 * @public
 */
/**
 * @internal
 */
const PBPointerEvents_Info = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.button !== undefined) {
            writer.uint32(8).int32(message.button);
        }
        if (message.hoverText !== undefined) {
            writer.uint32(18).string(message.hoverText);
        }
        if (message.maxDistance !== undefined) {
            writer.uint32(29).float(message.maxDistance);
        }
        if (message.showFeedback !== undefined) {
            writer.uint32(32).bool(message.showFeedback);
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBPointerEvents_Info();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.button = reader.int32();
                    break;
                case 2:
                    message.hoverText = reader.string();
                    break;
                case 3:
                    message.maxDistance = reader.float();
                    break;
                case 4:
                    message.showFeedback = reader.bool();
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};
function createBasePBPointerEvents_Entry() {
    return { eventType: 0, eventInfo: undefined };
}
/**
 * @public
 */
/**
 * @internal
 */
const PBPointerEvents_Entry = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.eventType !== 0) {
            writer.uint32(8).int32(message.eventType);
        }
        if (message.eventInfo !== undefined) {
            PBPointerEvents_Info.encode(message.eventInfo, writer.uint32(18).fork()).ldelim();
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBPointerEvents_Entry();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.eventType = reader.int32();
                    break;
                case 2:
                    message.eventInfo = PBPointerEvents_Info.decode(reader, reader.uint32());
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};

/**
 * @internal
 */
const PointerEventsSchema = {
    COMPONENT_ID: 1062,
    serialize(value, builder) {
        const writer = PBPointerEvents.encode(value);
        const buffer = new Uint8Array(writer.finish(), 0, writer.len);
        builder.writeBuffer(buffer, false);
    },
    deserialize(reader) {
        return PBPointerEvents.decode(reader.buffer(), reader.remainingBytes());
    },
    create() {
        // TODO: this is a hack.
        return PBPointerEvents.decode(new Uint8Array());
    },
    jsonSchema: {
        type: "object",
        properties: {},
        serializationType: "protocol-buffer",
        protocolBuffer: "PBPointerEvents"
    }
};

/* eslint-disable */
function createBasePBRaycastResult() {
    return { timestamp: 0, origin: undefined, direction: undefined, hits: [] };
}
/**
 * @public
 */
/**
 * @internal
 */
const PBRaycastResult = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.timestamp !== 0) {
            writer.uint32(8).int32(message.timestamp);
        }
        if (message.origin !== undefined) {
            Vector3$1.encode(message.origin, writer.uint32(18).fork()).ldelim();
        }
        if (message.direction !== undefined) {
            Vector3$1.encode(message.direction, writer.uint32(26).fork()).ldelim();
        }
        for (const v of message.hits) {
            RaycastHit.encode(v, writer.uint32(34).fork()).ldelim();
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBRaycastResult();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.timestamp = reader.int32();
                    break;
                case 2:
                    message.origin = Vector3$1.decode(reader, reader.uint32());
                    break;
                case 3:
                    message.direction = Vector3$1.decode(reader, reader.uint32());
                    break;
                case 4:
                    message.hits.push(RaycastHit.decode(reader, reader.uint32()));
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};
function createBaseRaycastHit() {
    return {
        position: undefined,
        origin: undefined,
        direction: undefined,
        normalHit: undefined,
        length: 0,
        meshName: undefined,
        entityId: undefined,
    };
}
/**
 * @public
 */
/**
 * @internal
 */
const RaycastHit = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.position !== undefined) {
            Vector3$1.encode(message.position, writer.uint32(10).fork()).ldelim();
        }
        if (message.origin !== undefined) {
            Vector3$1.encode(message.origin, writer.uint32(18).fork()).ldelim();
        }
        if (message.direction !== undefined) {
            Vector3$1.encode(message.direction, writer.uint32(26).fork()).ldelim();
        }
        if (message.normalHit !== undefined) {
            Vector3$1.encode(message.normalHit, writer.uint32(34).fork()).ldelim();
        }
        if (message.length !== 0) {
            writer.uint32(45).float(message.length);
        }
        if (message.meshName !== undefined) {
            writer.uint32(50).string(message.meshName);
        }
        if (message.entityId !== undefined) {
            writer.uint32(56).int64(message.entityId);
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBaseRaycastHit();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.position = Vector3$1.decode(reader, reader.uint32());
                    break;
                case 2:
                    message.origin = Vector3$1.decode(reader, reader.uint32());
                    break;
                case 3:
                    message.direction = Vector3$1.decode(reader, reader.uint32());
                    break;
                case 4:
                    message.normalHit = Vector3$1.decode(reader, reader.uint32());
                    break;
                case 5:
                    message.length = reader.float();
                    break;
                case 6:
                    message.meshName = reader.string();
                    break;
                case 7:
                    message.entityId = longToNumber(reader.int64());
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};
var tsProtoGlobalThis = (() => {
    if (typeof globalThis !== "undefined") {
        return globalThis;
    }
    if (typeof self !== "undefined") {
        return self;
    }
    if (typeof global !== "undefined") {
        return global;
    }
    throw "Unable to locate global object";
})();
function longToNumber(long) {
    if (long.gt(Number.MAX_SAFE_INTEGER)) {
        throw new tsProtoGlobalThis.Error("Value is larger than Number.MAX_SAFE_INTEGER");
    }
    return long.toNumber();
}
if (_m0.util.Long !== Long) {
    _m0.util.Long = Long;
    _m0.configure();
}

/* eslint-disable */
function createBasePBPointerEventsResult() {
    return { button: 0, hit: undefined, state: 0, timestamp: 0, analog: undefined };
}
/**
 * @public
 */
/**
 * @internal
 */
const PBPointerEventsResult = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.button !== 0) {
            writer.uint32(8).int32(message.button);
        }
        if (message.hit !== undefined) {
            RaycastHit.encode(message.hit, writer.uint32(18).fork()).ldelim();
        }
        if (message.state !== 0) {
            writer.uint32(32).int32(message.state);
        }
        if (message.timestamp !== 0) {
            writer.uint32(40).int32(message.timestamp);
        }
        if (message.analog !== undefined) {
            writer.uint32(53).float(message.analog);
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBPointerEventsResult();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.button = reader.int32();
                    break;
                case 2:
                    message.hit = RaycastHit.decode(reader, reader.uint32());
                    break;
                case 4:
                    message.state = reader.int32();
                    break;
                case 5:
                    message.timestamp = reader.int32();
                    break;
                case 6:
                    message.analog = reader.float();
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};

/**
 * @internal
 */
const PointerEventsResultSchema = {
    COMPONENT_ID: 1063,
    serialize(value, builder) {
        const writer = PBPointerEventsResult.encode(value);
        const buffer = new Uint8Array(writer.finish(), 0, writer.len);
        builder.writeBuffer(buffer, false);
    },
    deserialize(reader) {
        return PBPointerEventsResult.decode(reader.buffer(), reader.remainingBytes());
    },
    create() {
        // TODO: this is a hack.
        return PBPointerEventsResult.decode(new Uint8Array());
    },
    jsonSchema: {
        type: "object",
        properties: {},
        serializationType: "protocol-buffer",
        protocolBuffer: "PBPointerEventsResult"
    }
};

/* eslint-disable */
function createBasePBPointerLock() {
    return { isPointerLocked: false };
}
/**
 * @public
 */
/**
 * @internal
 */
const PBPointerLock = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.isPointerLocked === true) {
            writer.uint32(8).bool(message.isPointerLocked);
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBPointerLock();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.isPointerLocked = reader.bool();
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};

/**
 * @internal
 */
const PointerLockSchema = {
    COMPONENT_ID: 1074,
    serialize(value, builder) {
        const writer = PBPointerLock.encode(value);
        const buffer = new Uint8Array(writer.finish(), 0, writer.len);
        builder.writeBuffer(buffer, false);
    },
    deserialize(reader) {
        return PBPointerLock.decode(reader.buffer(), reader.remainingBytes());
    },
    create() {
        // TODO: this is a hack.
        return PBPointerLock.decode(new Uint8Array());
    },
    jsonSchema: {
        type: "object",
        properties: {},
        serializationType: "protocol-buffer",
        protocolBuffer: "PBPointerLock"
    }
};

/* eslint-disable */
/**
 * @public
 */
var RaycastQueryType;
(function (RaycastQueryType) {
    RaycastQueryType[RaycastQueryType["RQT_HIT_FIRST"] = 0] = "RQT_HIT_FIRST";
    RaycastQueryType[RaycastQueryType["RQT_QUERY_ALL"] = 1] = "RQT_QUERY_ALL";
})(RaycastQueryType || (RaycastQueryType = {}));
function createBasePBRaycast() {
    return { origin: undefined, direction: undefined, maxDistance: 0, queryType: 0 };
}
/**
 * @public
 */
/**
 * @internal
 */
const PBRaycast = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.origin !== undefined) {
            Vector3$1.encode(message.origin, writer.uint32(18).fork()).ldelim();
        }
        if (message.direction !== undefined) {
            Vector3$1.encode(message.direction, writer.uint32(26).fork()).ldelim();
        }
        if (message.maxDistance !== 0) {
            writer.uint32(37).float(message.maxDistance);
        }
        if (message.queryType !== 0) {
            writer.uint32(40).int32(message.queryType);
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBRaycast();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 2:
                    message.origin = Vector3$1.decode(reader, reader.uint32());
                    break;
                case 3:
                    message.direction = Vector3$1.decode(reader, reader.uint32());
                    break;
                case 4:
                    message.maxDistance = reader.float();
                    break;
                case 5:
                    message.queryType = reader.int32();
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};

/**
 * @internal
 */
const RaycastSchema = {
    COMPONENT_ID: 1067,
    serialize(value, builder) {
        const writer = PBRaycast.encode(value);
        const buffer = new Uint8Array(writer.finish(), 0, writer.len);
        builder.writeBuffer(buffer, false);
    },
    deserialize(reader) {
        return PBRaycast.decode(reader.buffer(), reader.remainingBytes());
    },
    create() {
        // TODO: this is a hack.
        return PBRaycast.decode(new Uint8Array());
    },
    jsonSchema: {
        type: "object",
        properties: {},
        serializationType: "protocol-buffer",
        protocolBuffer: "PBRaycast"
    }
};

/**
 * @internal
 */
const RaycastResultSchema = {
    COMPONENT_ID: 1068,
    serialize(value, builder) {
        const writer = PBRaycastResult.encode(value);
        const buffer = new Uint8Array(writer.finish(), 0, writer.len);
        builder.writeBuffer(buffer, false);
    },
    deserialize(reader) {
        return PBRaycastResult.decode(reader.buffer(), reader.remainingBytes());
    },
    create() {
        // TODO: this is a hack.
        return PBRaycastResult.decode(new Uint8Array());
    },
    jsonSchema: {
        type: "object",
        properties: {},
        serializationType: "protocol-buffer",
        protocolBuffer: "PBRaycastResult"
    }
};

/* eslint-disable */
function createBasePBTextShape() {
    return {
        text: "",
        font: undefined,
        fontSize: undefined,
        fontAutoSize: undefined,
        textAlign: undefined,
        width: undefined,
        height: undefined,
        paddingTop: undefined,
        paddingRight: undefined,
        paddingBottom: undefined,
        paddingLeft: undefined,
        lineSpacing: undefined,
        lineCount: undefined,
        textWrapping: undefined,
        shadowBlur: undefined,
        shadowOffsetX: undefined,
        shadowOffsetY: undefined,
        outlineWidth: undefined,
        shadowColor: undefined,
        outlineColor: undefined,
        textColor: undefined,
    };
}
/**
 * @public
 */
/**
 * @internal
 */
const PBTextShape = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.text !== "") {
            writer.uint32(10).string(message.text);
        }
        if (message.font !== undefined) {
            writer.uint32(16).int32(message.font);
        }
        if (message.fontSize !== undefined) {
            writer.uint32(29).float(message.fontSize);
        }
        if (message.fontAutoSize !== undefined) {
            writer.uint32(32).bool(message.fontAutoSize);
        }
        if (message.textAlign !== undefined) {
            writer.uint32(40).int32(message.textAlign);
        }
        if (message.width !== undefined) {
            writer.uint32(53).float(message.width);
        }
        if (message.height !== undefined) {
            writer.uint32(61).float(message.height);
        }
        if (message.paddingTop !== undefined) {
            writer.uint32(69).float(message.paddingTop);
        }
        if (message.paddingRight !== undefined) {
            writer.uint32(77).float(message.paddingRight);
        }
        if (message.paddingBottom !== undefined) {
            writer.uint32(85).float(message.paddingBottom);
        }
        if (message.paddingLeft !== undefined) {
            writer.uint32(93).float(message.paddingLeft);
        }
        if (message.lineSpacing !== undefined) {
            writer.uint32(101).float(message.lineSpacing);
        }
        if (message.lineCount !== undefined) {
            writer.uint32(104).int32(message.lineCount);
        }
        if (message.textWrapping !== undefined) {
            writer.uint32(112).bool(message.textWrapping);
        }
        if (message.shadowBlur !== undefined) {
            writer.uint32(125).float(message.shadowBlur);
        }
        if (message.shadowOffsetX !== undefined) {
            writer.uint32(133).float(message.shadowOffsetX);
        }
        if (message.shadowOffsetY !== undefined) {
            writer.uint32(141).float(message.shadowOffsetY);
        }
        if (message.outlineWidth !== undefined) {
            writer.uint32(149).float(message.outlineWidth);
        }
        if (message.shadowColor !== undefined) {
            Color3.encode(message.shadowColor, writer.uint32(154).fork()).ldelim();
        }
        if (message.outlineColor !== undefined) {
            Color3.encode(message.outlineColor, writer.uint32(162).fork()).ldelim();
        }
        if (message.textColor !== undefined) {
            Color4$1.encode(message.textColor, writer.uint32(170).fork()).ldelim();
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBTextShape();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.text = reader.string();
                    break;
                case 2:
                    message.font = reader.int32();
                    break;
                case 3:
                    message.fontSize = reader.float();
                    break;
                case 4:
                    message.fontAutoSize = reader.bool();
                    break;
                case 5:
                    message.textAlign = reader.int32();
                    break;
                case 6:
                    message.width = reader.float();
                    break;
                case 7:
                    message.height = reader.float();
                    break;
                case 8:
                    message.paddingTop = reader.float();
                    break;
                case 9:
                    message.paddingRight = reader.float();
                    break;
                case 10:
                    message.paddingBottom = reader.float();
                    break;
                case 11:
                    message.paddingLeft = reader.float();
                    break;
                case 12:
                    message.lineSpacing = reader.float();
                    break;
                case 13:
                    message.lineCount = reader.int32();
                    break;
                case 14:
                    message.textWrapping = reader.bool();
                    break;
                case 15:
                    message.shadowBlur = reader.float();
                    break;
                case 16:
                    message.shadowOffsetX = reader.float();
                    break;
                case 17:
                    message.shadowOffsetY = reader.float();
                    break;
                case 18:
                    message.outlineWidth = reader.float();
                    break;
                case 19:
                    message.shadowColor = Color3.decode(reader, reader.uint32());
                    break;
                case 20:
                    message.outlineColor = Color3.decode(reader, reader.uint32());
                    break;
                case 21:
                    message.textColor = Color4$1.decode(reader, reader.uint32());
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};

/**
 * @internal
 */
const TextShapeSchema = {
    COMPONENT_ID: 1030,
    serialize(value, builder) {
        const writer = PBTextShape.encode(value);
        const buffer = new Uint8Array(writer.finish(), 0, writer.len);
        builder.writeBuffer(buffer, false);
    },
    deserialize(reader) {
        return PBTextShape.decode(reader.buffer(), reader.remainingBytes());
    },
    create() {
        // TODO: this is a hack.
        return PBTextShape.decode(new Uint8Array());
    },
    jsonSchema: {
        type: "object",
        properties: {},
        serializationType: "protocol-buffer",
        protocolBuffer: "PBTextShape"
    }
};

/* eslint-disable */
function createBaseBorderRect() {
    return { top: 0, left: 0, right: 0, bottom: 0 };
}
/**
 * @public
 */
/**
 * @internal
 */
const BorderRect = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.top !== 0) {
            writer.uint32(13).float(message.top);
        }
        if (message.left !== 0) {
            writer.uint32(21).float(message.left);
        }
        if (message.right !== 0) {
            writer.uint32(29).float(message.right);
        }
        if (message.bottom !== 0) {
            writer.uint32(37).float(message.bottom);
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBaseBorderRect();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.top = reader.float();
                    break;
                case 2:
                    message.left = reader.float();
                    break;
                case 3:
                    message.right = reader.float();
                    break;
                case 4:
                    message.bottom = reader.float();
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};

/* eslint-disable */
/**
 * @public
 */
var BackgroundTextureMode;
(function (BackgroundTextureMode) {
    /**
     * NINE_SLICES - https://docs.unity3d.com/Manual/UIE-USS-SupportedProperties.html (Slicing section)
     * https://forum.unity.com/threads/how-does-slicing-in-ui-tookkit-works.1235863/
     * https://docs.unity3d.com/Manual/9SliceSprites.html
     * https://developer.mozilla.org/en-US/docs/Web/CSS/border-image-slice
     */
    BackgroundTextureMode[BackgroundTextureMode["NINE_SLICES"] = 0] = "NINE_SLICES";
    /**
     * CENTER - CENTER enables the texture to be rendered centered in relation to the
     * element. If the element is smaller than the texture then the background
     * should use the element as stencil to cut off the out-of-bounds area
     */
    BackgroundTextureMode[BackgroundTextureMode["CENTER"] = 1] = "CENTER";
    /**
     * STRETCH - STRETCH enables the texture to cover all the area of the container,
     * adopting its aspect ratio.
     */
    BackgroundTextureMode[BackgroundTextureMode["STRETCH"] = 2] = "STRETCH";
})(BackgroundTextureMode || (BackgroundTextureMode = {}));
function createBasePBUiBackground() {
    return { color: undefined, texture: undefined, textureMode: 0, textureSlices: undefined, uvs: [] };
}
/**
 * @public
 */
/**
 * @internal
 */
const PBUiBackground = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.color !== undefined) {
            Color4$1.encode(message.color, writer.uint32(10).fork()).ldelim();
        }
        if (message.texture !== undefined) {
            TextureUnion.encode(message.texture, writer.uint32(18).fork()).ldelim();
        }
        if (message.textureMode !== 0) {
            writer.uint32(24).int32(message.textureMode);
        }
        if (message.textureSlices !== undefined) {
            BorderRect.encode(message.textureSlices, writer.uint32(34).fork()).ldelim();
        }
        writer.uint32(42).fork();
        for (const v of message.uvs) {
            writer.float(v);
        }
        writer.ldelim();
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBUiBackground();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.color = Color4$1.decode(reader, reader.uint32());
                    break;
                case 2:
                    message.texture = TextureUnion.decode(reader, reader.uint32());
                    break;
                case 3:
                    message.textureMode = reader.int32();
                    break;
                case 4:
                    message.textureSlices = BorderRect.decode(reader, reader.uint32());
                    break;
                case 5:
                    if ((tag & 7) === 2) {
                        const end2 = reader.uint32() + reader.pos;
                        while (reader.pos < end2) {
                            message.uvs.push(reader.float());
                        }
                    }
                    else {
                        message.uvs.push(reader.float());
                    }
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};

/**
 * @internal
 */
const UiBackgroundSchema = {
    COMPONENT_ID: 1053,
    serialize(value, builder) {
        const writer = PBUiBackground.encode(value);
        const buffer = new Uint8Array(writer.finish(), 0, writer.len);
        builder.writeBuffer(buffer, false);
    },
    deserialize(reader) {
        return PBUiBackground.decode(reader.buffer(), reader.remainingBytes());
    },
    create() {
        // TODO: this is a hack.
        return PBUiBackground.decode(new Uint8Array());
    },
    jsonSchema: {
        type: "object",
        properties: {},
        serializationType: "protocol-buffer",
        protocolBuffer: "PBUiBackground"
    }
};

/* eslint-disable */
function createBasePBUiDropdown() {
    return {
        acceptEmpty: false,
        emptyLabel: undefined,
        options: [],
        selectedIndex: undefined,
        disabled: false,
        color: undefined,
        textAlign: undefined,
        font: undefined,
        fontSize: undefined,
    };
}
/**
 * @public
 */
/**
 * @internal
 */
const PBUiDropdown = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.acceptEmpty === true) {
            writer.uint32(8).bool(message.acceptEmpty);
        }
        if (message.emptyLabel !== undefined) {
            writer.uint32(18).string(message.emptyLabel);
        }
        for (const v of message.options) {
            writer.uint32(26).string(v);
        }
        if (message.selectedIndex !== undefined) {
            writer.uint32(32).int32(message.selectedIndex);
        }
        if (message.disabled === true) {
            writer.uint32(40).bool(message.disabled);
        }
        if (message.color !== undefined) {
            Color4$1.encode(message.color, writer.uint32(50).fork()).ldelim();
        }
        if (message.textAlign !== undefined) {
            writer.uint32(80).int32(message.textAlign);
        }
        if (message.font !== undefined) {
            writer.uint32(88).int32(message.font);
        }
        if (message.fontSize !== undefined) {
            writer.uint32(96).int32(message.fontSize);
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBUiDropdown();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.acceptEmpty = reader.bool();
                    break;
                case 2:
                    message.emptyLabel = reader.string();
                    break;
                case 3:
                    message.options.push(reader.string());
                    break;
                case 4:
                    message.selectedIndex = reader.int32();
                    break;
                case 5:
                    message.disabled = reader.bool();
                    break;
                case 6:
                    message.color = Color4$1.decode(reader, reader.uint32());
                    break;
                case 10:
                    message.textAlign = reader.int32();
                    break;
                case 11:
                    message.font = reader.int32();
                    break;
                case 12:
                    message.fontSize = reader.int32();
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};

/**
 * @internal
 */
const UiDropdownSchema = {
    COMPONENT_ID: 1094,
    serialize(value, builder) {
        const writer = PBUiDropdown.encode(value);
        const buffer = new Uint8Array(writer.finish(), 0, writer.len);
        builder.writeBuffer(buffer, false);
    },
    deserialize(reader) {
        return PBUiDropdown.decode(reader.buffer(), reader.remainingBytes());
    },
    create() {
        // TODO: this is a hack.
        return PBUiDropdown.decode(new Uint8Array());
    },
    jsonSchema: {
        type: "object",
        properties: {},
        serializationType: "protocol-buffer",
        protocolBuffer: "PBUiDropdown"
    }
};

/* eslint-disable */
function createBasePBUiDropdownResult() {
    return { value: 0 };
}
/**
 * @public
 */
/**
 * @internal
 */
const PBUiDropdownResult = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.value !== 0) {
            writer.uint32(8).int32(message.value);
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBUiDropdownResult();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.value = reader.int32();
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};

/**
 * @internal
 */
const UiDropdownResultSchema = {
    COMPONENT_ID: 1096,
    serialize(value, builder) {
        const writer = PBUiDropdownResult.encode(value);
        const buffer = new Uint8Array(writer.finish(), 0, writer.len);
        builder.writeBuffer(buffer, false);
    },
    deserialize(reader) {
        return PBUiDropdownResult.decode(reader.buffer(), reader.remainingBytes());
    },
    create() {
        // TODO: this is a hack.
        return PBUiDropdownResult.decode(new Uint8Array());
    },
    jsonSchema: {
        type: "object",
        properties: {},
        serializationType: "protocol-buffer",
        protocolBuffer: "PBUiDropdownResult"
    }
};

/* eslint-disable */
function createBasePBUiInput() {
    return {
        placeholder: "",
        color: undefined,
        placeholderColor: undefined,
        disabled: false,
        textAlign: undefined,
        font: undefined,
        fontSize: undefined,
    };
}
/**
 * @public
 */
/**
 * @internal
 */
const PBUiInput = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.placeholder !== "") {
            writer.uint32(10).string(message.placeholder);
        }
        if (message.color !== undefined) {
            Color4$1.encode(message.color, writer.uint32(18).fork()).ldelim();
        }
        if (message.placeholderColor !== undefined) {
            Color4$1.encode(message.placeholderColor, writer.uint32(26).fork()).ldelim();
        }
        if (message.disabled === true) {
            writer.uint32(32).bool(message.disabled);
        }
        if (message.textAlign !== undefined) {
            writer.uint32(80).int32(message.textAlign);
        }
        if (message.font !== undefined) {
            writer.uint32(88).int32(message.font);
        }
        if (message.fontSize !== undefined) {
            writer.uint32(96).int32(message.fontSize);
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBUiInput();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.placeholder = reader.string();
                    break;
                case 2:
                    message.color = Color4$1.decode(reader, reader.uint32());
                    break;
                case 3:
                    message.placeholderColor = Color4$1.decode(reader, reader.uint32());
                    break;
                case 4:
                    message.disabled = reader.bool();
                    break;
                case 10:
                    message.textAlign = reader.int32();
                    break;
                case 11:
                    message.font = reader.int32();
                    break;
                case 12:
                    message.fontSize = reader.int32();
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};

/**
 * @internal
 */
const UiInputSchema = {
    COMPONENT_ID: 1093,
    serialize(value, builder) {
        const writer = PBUiInput.encode(value);
        const buffer = new Uint8Array(writer.finish(), 0, writer.len);
        builder.writeBuffer(buffer, false);
    },
    deserialize(reader) {
        return PBUiInput.decode(reader.buffer(), reader.remainingBytes());
    },
    create() {
        // TODO: this is a hack.
        return PBUiInput.decode(new Uint8Array());
    },
    jsonSchema: {
        type: "object",
        properties: {},
        serializationType: "protocol-buffer",
        protocolBuffer: "PBUiInput"
    }
};

/* eslint-disable */
function createBasePBUiInputResult() {
    return { value: "" };
}
/**
 * @public
 */
/**
 * @internal
 */
const PBUiInputResult = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.value !== "") {
            writer.uint32(10).string(message.value);
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBUiInputResult();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.value = reader.string();
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};

/**
 * @internal
 */
const UiInputResultSchema = {
    COMPONENT_ID: 1095,
    serialize(value, builder) {
        const writer = PBUiInputResult.encode(value);
        const buffer = new Uint8Array(writer.finish(), 0, writer.len);
        builder.writeBuffer(buffer, false);
    },
    deserialize(reader) {
        return PBUiInputResult.decode(reader.buffer(), reader.remainingBytes());
    },
    create() {
        // TODO: this is a hack.
        return PBUiInputResult.decode(new Uint8Array());
    },
    jsonSchema: {
        type: "object",
        properties: {},
        serializationType: "protocol-buffer",
        protocolBuffer: "PBUiInputResult"
    }
};

/* eslint-disable */
function createBasePBUiText() {
    return { value: "", color: undefined, textAlign: undefined, font: undefined, fontSize: undefined };
}
/**
 * @public
 */
/**
 * @internal
 */
const PBUiText = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.value !== "") {
            writer.uint32(10).string(message.value);
        }
        if (message.color !== undefined) {
            Color4$1.encode(message.color, writer.uint32(18).fork()).ldelim();
        }
        if (message.textAlign !== undefined) {
            writer.uint32(24).int32(message.textAlign);
        }
        if (message.font !== undefined) {
            writer.uint32(32).int32(message.font);
        }
        if (message.fontSize !== undefined) {
            writer.uint32(40).int32(message.fontSize);
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBUiText();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.value = reader.string();
                    break;
                case 2:
                    message.color = Color4$1.decode(reader, reader.uint32());
                    break;
                case 3:
                    message.textAlign = reader.int32();
                    break;
                case 4:
                    message.font = reader.int32();
                    break;
                case 5:
                    message.fontSize = reader.int32();
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};

/**
 * @internal
 */
const UiTextSchema = {
    COMPONENT_ID: 1052,
    serialize(value, builder) {
        const writer = PBUiText.encode(value);
        const buffer = new Uint8Array(writer.finish(), 0, writer.len);
        builder.writeBuffer(buffer, false);
    },
    deserialize(reader) {
        return PBUiText.decode(reader.buffer(), reader.remainingBytes());
    },
    create() {
        // TODO: this is a hack.
        return PBUiText.decode(new Uint8Array());
    },
    jsonSchema: {
        type: "object",
        properties: {},
        serializationType: "protocol-buffer",
        protocolBuffer: "PBUiText"
    }
};

/* eslint-disable */
/**
 * @public
 */
var YGPositionType;
(function (YGPositionType) {
    YGPositionType[YGPositionType["YGPT_RELATIVE"] = 0] = "YGPT_RELATIVE";
    YGPositionType[YGPositionType["YGPT_ABSOLUTE"] = 1] = "YGPT_ABSOLUTE";
})(YGPositionType || (YGPositionType = {}));
/**
 * @public
 */
var YGAlign;
(function (YGAlign) {
    YGAlign[YGAlign["YGA_AUTO"] = 0] = "YGA_AUTO";
    YGAlign[YGAlign["YGA_FLEX_START"] = 1] = "YGA_FLEX_START";
    YGAlign[YGAlign["YGA_CENTER"] = 2] = "YGA_CENTER";
    YGAlign[YGAlign["YGA_FLEX_END"] = 3] = "YGA_FLEX_END";
    YGAlign[YGAlign["YGA_STRETCH"] = 4] = "YGA_STRETCH";
    YGAlign[YGAlign["YGA_BASELINE"] = 5] = "YGA_BASELINE";
    YGAlign[YGAlign["YGA_SPACE_BETWEEN"] = 6] = "YGA_SPACE_BETWEEN";
    YGAlign[YGAlign["YGA_SPACE_AROUND"] = 7] = "YGA_SPACE_AROUND";
})(YGAlign || (YGAlign = {}));
/**
 * @public
 */
var YGUnit;
(function (YGUnit) {
    YGUnit[YGUnit["YGU_UNDEFINED"] = 0] = "YGU_UNDEFINED";
    YGUnit[YGUnit["YGU_POINT"] = 1] = "YGU_POINT";
    YGUnit[YGUnit["YGU_PERCENT"] = 2] = "YGU_PERCENT";
    YGUnit[YGUnit["YGU_AUTO"] = 3] = "YGU_AUTO";
})(YGUnit || (YGUnit = {}));
/**
 * @public
 */
var YGFlexDirection;
(function (YGFlexDirection) {
    YGFlexDirection[YGFlexDirection["YGFD_ROW"] = 0] = "YGFD_ROW";
    YGFlexDirection[YGFlexDirection["YGFD_COLUMN"] = 1] = "YGFD_COLUMN";
    YGFlexDirection[YGFlexDirection["YGFD_COLUMN_REVERSE"] = 2] = "YGFD_COLUMN_REVERSE";
    YGFlexDirection[YGFlexDirection["YGFD_ROW_REVERSE"] = 3] = "YGFD_ROW_REVERSE";
})(YGFlexDirection || (YGFlexDirection = {}));
/**
 * @public
 */
var YGWrap;
(function (YGWrap) {
    YGWrap[YGWrap["YGW_NO_WRAP"] = 0] = "YGW_NO_WRAP";
    YGWrap[YGWrap["YGW_WRAP"] = 1] = "YGW_WRAP";
    YGWrap[YGWrap["YGW_WRAP_REVERSE"] = 2] = "YGW_WRAP_REVERSE";
})(YGWrap || (YGWrap = {}));
/**
 * @public
 */
var YGJustify;
(function (YGJustify) {
    YGJustify[YGJustify["YGJ_FLEX_START"] = 0] = "YGJ_FLEX_START";
    YGJustify[YGJustify["YGJ_CENTER"] = 1] = "YGJ_CENTER";
    YGJustify[YGJustify["YGJ_FLEX_END"] = 2] = "YGJ_FLEX_END";
    YGJustify[YGJustify["YGJ_SPACE_BETWEEN"] = 3] = "YGJ_SPACE_BETWEEN";
    YGJustify[YGJustify["YGJ_SPACE_AROUND"] = 4] = "YGJ_SPACE_AROUND";
    YGJustify[YGJustify["YGJ_SPACE_EVENLY"] = 5] = "YGJ_SPACE_EVENLY";
})(YGJustify || (YGJustify = {}));
/**
 * @public
 */
var YGOverflow;
(function (YGOverflow) {
    YGOverflow[YGOverflow["YGO_VISIBLE"] = 0] = "YGO_VISIBLE";
    YGOverflow[YGOverflow["YGO_HIDDEN"] = 1] = "YGO_HIDDEN";
    YGOverflow[YGOverflow["YGO_SCROLL"] = 2] = "YGO_SCROLL";
})(YGOverflow || (YGOverflow = {}));
/**
 * @public
 */
var YGDisplay;
(function (YGDisplay) {
    YGDisplay[YGDisplay["YGD_FLEX"] = 0] = "YGD_FLEX";
    YGDisplay[YGDisplay["YGD_NONE"] = 1] = "YGD_NONE";
})(YGDisplay || (YGDisplay = {}));
/**
 * @public
 */
var YGEdge;
(function (YGEdge) {
    YGEdge[YGEdge["YGE_LEFT"] = 0] = "YGE_LEFT";
    YGEdge[YGEdge["YGE_TOP"] = 1] = "YGE_TOP";
    YGEdge[YGEdge["YGE_RIGHT"] = 2] = "YGE_RIGHT";
    YGEdge[YGEdge["YGE_BOTTOM"] = 3] = "YGE_BOTTOM";
    YGEdge[YGEdge["YGE_START"] = 4] = "YGE_START";
    YGEdge[YGEdge["YGE_END"] = 5] = "YGE_END";
    YGEdge[YGEdge["YGE_HORIZONTAL"] = 6] = "YGE_HORIZONTAL";
    YGEdge[YGEdge["YGE_VERTICAL"] = 7] = "YGE_VERTICAL";
    YGEdge[YGEdge["YGE_ALL"] = 8] = "YGE_ALL";
})(YGEdge || (YGEdge = {}));
function createBasePBUiTransform() {
    return {
        parent: 0,
        rightOf: 0,
        alignContent: undefined,
        alignItems: undefined,
        flexWrap: undefined,
        flexShrink: undefined,
        positionType: 0,
        alignSelf: 0,
        flexDirection: 0,
        justifyContent: 0,
        overflow: 0,
        display: 0,
        flexBasisUnit: 0,
        flexBasis: 0,
        flexGrow: 0,
        widthUnit: 0,
        width: 0,
        heightUnit: 0,
        height: 0,
        minWidthUnit: 0,
        minWidth: 0,
        minHeightUnit: 0,
        minHeight: 0,
        maxWidthUnit: 0,
        maxWidth: 0,
        maxHeightUnit: 0,
        maxHeight: 0,
        positionLeftUnit: 0,
        positionLeft: 0,
        positionTopUnit: 0,
        positionTop: 0,
        positionRightUnit: 0,
        positionRight: 0,
        positionBottomUnit: 0,
        positionBottom: 0,
        marginLeftUnit: 0,
        marginLeft: 0,
        marginTopUnit: 0,
        marginTop: 0,
        marginRightUnit: 0,
        marginRight: 0,
        marginBottomUnit: 0,
        marginBottom: 0,
        paddingLeftUnit: 0,
        paddingLeft: 0,
        paddingTopUnit: 0,
        paddingTop: 0,
        paddingRightUnit: 0,
        paddingRight: 0,
        paddingBottomUnit: 0,
        paddingBottom: 0,
    };
}
/**
 * @public
 */
/**
 * @internal
 */
const PBUiTransform = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.parent !== 0) {
            writer.uint32(8).int32(message.parent);
        }
        if (message.rightOf !== 0) {
            writer.uint32(16).int32(message.rightOf);
        }
        if (message.alignContent !== undefined) {
            writer.uint32(24).int32(message.alignContent);
        }
        if (message.alignItems !== undefined) {
            writer.uint32(32).int32(message.alignItems);
        }
        if (message.flexWrap !== undefined) {
            writer.uint32(40).int32(message.flexWrap);
        }
        if (message.flexShrink !== undefined) {
            writer.uint32(53).float(message.flexShrink);
        }
        if (message.positionType !== 0) {
            writer.uint32(56).int32(message.positionType);
        }
        if (message.alignSelf !== 0) {
            writer.uint32(64).int32(message.alignSelf);
        }
        if (message.flexDirection !== 0) {
            writer.uint32(72).int32(message.flexDirection);
        }
        if (message.justifyContent !== 0) {
            writer.uint32(80).int32(message.justifyContent);
        }
        if (message.overflow !== 0) {
            writer.uint32(88).int32(message.overflow);
        }
        if (message.display !== 0) {
            writer.uint32(96).int32(message.display);
        }
        if (message.flexBasisUnit !== 0) {
            writer.uint32(104).int32(message.flexBasisUnit);
        }
        if (message.flexBasis !== 0) {
            writer.uint32(117).float(message.flexBasis);
        }
        if (message.flexGrow !== 0) {
            writer.uint32(125).float(message.flexGrow);
        }
        if (message.widthUnit !== 0) {
            writer.uint32(128).int32(message.widthUnit);
        }
        if (message.width !== 0) {
            writer.uint32(141).float(message.width);
        }
        if (message.heightUnit !== 0) {
            writer.uint32(144).int32(message.heightUnit);
        }
        if (message.height !== 0) {
            writer.uint32(157).float(message.height);
        }
        if (message.minWidthUnit !== 0) {
            writer.uint32(160).int32(message.minWidthUnit);
        }
        if (message.minWidth !== 0) {
            writer.uint32(173).float(message.minWidth);
        }
        if (message.minHeightUnit !== 0) {
            writer.uint32(176).int32(message.minHeightUnit);
        }
        if (message.minHeight !== 0) {
            writer.uint32(189).float(message.minHeight);
        }
        if (message.maxWidthUnit !== 0) {
            writer.uint32(192).int32(message.maxWidthUnit);
        }
        if (message.maxWidth !== 0) {
            writer.uint32(205).float(message.maxWidth);
        }
        if (message.maxHeightUnit !== 0) {
            writer.uint32(208).int32(message.maxHeightUnit);
        }
        if (message.maxHeight !== 0) {
            writer.uint32(221).float(message.maxHeight);
        }
        if (message.positionLeftUnit !== 0) {
            writer.uint32(224).int32(message.positionLeftUnit);
        }
        if (message.positionLeft !== 0) {
            writer.uint32(237).float(message.positionLeft);
        }
        if (message.positionTopUnit !== 0) {
            writer.uint32(240).int32(message.positionTopUnit);
        }
        if (message.positionTop !== 0) {
            writer.uint32(253).float(message.positionTop);
        }
        if (message.positionRightUnit !== 0) {
            writer.uint32(256).int32(message.positionRightUnit);
        }
        if (message.positionRight !== 0) {
            writer.uint32(269).float(message.positionRight);
        }
        if (message.positionBottomUnit !== 0) {
            writer.uint32(272).int32(message.positionBottomUnit);
        }
        if (message.positionBottom !== 0) {
            writer.uint32(285).float(message.positionBottom);
        }
        if (message.marginLeftUnit !== 0) {
            writer.uint32(288).int32(message.marginLeftUnit);
        }
        if (message.marginLeft !== 0) {
            writer.uint32(301).float(message.marginLeft);
        }
        if (message.marginTopUnit !== 0) {
            writer.uint32(304).int32(message.marginTopUnit);
        }
        if (message.marginTop !== 0) {
            writer.uint32(317).float(message.marginTop);
        }
        if (message.marginRightUnit !== 0) {
            writer.uint32(320).int32(message.marginRightUnit);
        }
        if (message.marginRight !== 0) {
            writer.uint32(333).float(message.marginRight);
        }
        if (message.marginBottomUnit !== 0) {
            writer.uint32(336).int32(message.marginBottomUnit);
        }
        if (message.marginBottom !== 0) {
            writer.uint32(349).float(message.marginBottom);
        }
        if (message.paddingLeftUnit !== 0) {
            writer.uint32(352).int32(message.paddingLeftUnit);
        }
        if (message.paddingLeft !== 0) {
            writer.uint32(365).float(message.paddingLeft);
        }
        if (message.paddingTopUnit !== 0) {
            writer.uint32(368).int32(message.paddingTopUnit);
        }
        if (message.paddingTop !== 0) {
            writer.uint32(381).float(message.paddingTop);
        }
        if (message.paddingRightUnit !== 0) {
            writer.uint32(384).int32(message.paddingRightUnit);
        }
        if (message.paddingRight !== 0) {
            writer.uint32(397).float(message.paddingRight);
        }
        if (message.paddingBottomUnit !== 0) {
            writer.uint32(400).int32(message.paddingBottomUnit);
        }
        if (message.paddingBottom !== 0) {
            writer.uint32(413).float(message.paddingBottom);
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBUiTransform();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.parent = reader.int32();
                    break;
                case 2:
                    message.rightOf = reader.int32();
                    break;
                case 3:
                    message.alignContent = reader.int32();
                    break;
                case 4:
                    message.alignItems = reader.int32();
                    break;
                case 5:
                    message.flexWrap = reader.int32();
                    break;
                case 6:
                    message.flexShrink = reader.float();
                    break;
                case 7:
                    message.positionType = reader.int32();
                    break;
                case 8:
                    message.alignSelf = reader.int32();
                    break;
                case 9:
                    message.flexDirection = reader.int32();
                    break;
                case 10:
                    message.justifyContent = reader.int32();
                    break;
                case 11:
                    message.overflow = reader.int32();
                    break;
                case 12:
                    message.display = reader.int32();
                    break;
                case 13:
                    message.flexBasisUnit = reader.int32();
                    break;
                case 14:
                    message.flexBasis = reader.float();
                    break;
                case 15:
                    message.flexGrow = reader.float();
                    break;
                case 16:
                    message.widthUnit = reader.int32();
                    break;
                case 17:
                    message.width = reader.float();
                    break;
                case 18:
                    message.heightUnit = reader.int32();
                    break;
                case 19:
                    message.height = reader.float();
                    break;
                case 20:
                    message.minWidthUnit = reader.int32();
                    break;
                case 21:
                    message.minWidth = reader.float();
                    break;
                case 22:
                    message.minHeightUnit = reader.int32();
                    break;
                case 23:
                    message.minHeight = reader.float();
                    break;
                case 24:
                    message.maxWidthUnit = reader.int32();
                    break;
                case 25:
                    message.maxWidth = reader.float();
                    break;
                case 26:
                    message.maxHeightUnit = reader.int32();
                    break;
                case 27:
                    message.maxHeight = reader.float();
                    break;
                case 28:
                    message.positionLeftUnit = reader.int32();
                    break;
                case 29:
                    message.positionLeft = reader.float();
                    break;
                case 30:
                    message.positionTopUnit = reader.int32();
                    break;
                case 31:
                    message.positionTop = reader.float();
                    break;
                case 32:
                    message.positionRightUnit = reader.int32();
                    break;
                case 33:
                    message.positionRight = reader.float();
                    break;
                case 34:
                    message.positionBottomUnit = reader.int32();
                    break;
                case 35:
                    message.positionBottom = reader.float();
                    break;
                case 36:
                    message.marginLeftUnit = reader.int32();
                    break;
                case 37:
                    message.marginLeft = reader.float();
                    break;
                case 38:
                    message.marginTopUnit = reader.int32();
                    break;
                case 39:
                    message.marginTop = reader.float();
                    break;
                case 40:
                    message.marginRightUnit = reader.int32();
                    break;
                case 41:
                    message.marginRight = reader.float();
                    break;
                case 42:
                    message.marginBottomUnit = reader.int32();
                    break;
                case 43:
                    message.marginBottom = reader.float();
                    break;
                case 44:
                    message.paddingLeftUnit = reader.int32();
                    break;
                case 45:
                    message.paddingLeft = reader.float();
                    break;
                case 46:
                    message.paddingTopUnit = reader.int32();
                    break;
                case 47:
                    message.paddingTop = reader.float();
                    break;
                case 48:
                    message.paddingRightUnit = reader.int32();
                    break;
                case 49:
                    message.paddingRight = reader.float();
                    break;
                case 50:
                    message.paddingBottomUnit = reader.int32();
                    break;
                case 51:
                    message.paddingBottom = reader.float();
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};

/**
 * @internal
 */
const UiTransformSchema = {
    COMPONENT_ID: 1050,
    serialize(value, builder) {
        const writer = PBUiTransform.encode(value);
        const buffer = new Uint8Array(writer.finish(), 0, writer.len);
        builder.writeBuffer(buffer, false);
    },
    deserialize(reader) {
        return PBUiTransform.decode(reader.buffer(), reader.remainingBytes());
    },
    create() {
        // TODO: this is a hack.
        return PBUiTransform.decode(new Uint8Array());
    },
    jsonSchema: {
        type: "object",
        properties: {},
        serializationType: "protocol-buffer",
        protocolBuffer: "PBUiTransform"
    }
};

/* eslint-disable */
function createBasePBVideoPlayer() {
    return {
        src: "",
        playing: undefined,
        position: undefined,
        volume: undefined,
        playbackRate: undefined,
        loop: undefined,
    };
}
/**
 * @public
 */
/**
 * @internal
 */
const PBVideoPlayer = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.src !== "") {
            writer.uint32(10).string(message.src);
        }
        if (message.playing !== undefined) {
            writer.uint32(16).bool(message.playing);
        }
        if (message.position !== undefined) {
            writer.uint32(29).float(message.position);
        }
        if (message.volume !== undefined) {
            writer.uint32(37).float(message.volume);
        }
        if (message.playbackRate !== undefined) {
            writer.uint32(45).float(message.playbackRate);
        }
        if (message.loop !== undefined) {
            writer.uint32(48).bool(message.loop);
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBVideoPlayer();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.src = reader.string();
                    break;
                case 2:
                    message.playing = reader.bool();
                    break;
                case 3:
                    message.position = reader.float();
                    break;
                case 4:
                    message.volume = reader.float();
                    break;
                case 5:
                    message.playbackRate = reader.float();
                    break;
                case 6:
                    message.loop = reader.bool();
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};

/**
 * @internal
 */
const VideoPlayerSchema = {
    COMPONENT_ID: 1043,
    serialize(value, builder) {
        const writer = PBVideoPlayer.encode(value);
        const buffer = new Uint8Array(writer.finish(), 0, writer.len);
        builder.writeBuffer(buffer, false);
    },
    deserialize(reader) {
        return PBVideoPlayer.decode(reader.buffer(), reader.remainingBytes());
    },
    create() {
        // TODO: this is a hack.
        return PBVideoPlayer.decode(new Uint8Array());
    },
    jsonSchema: {
        type: "object",
        properties: {},
        serializationType: "protocol-buffer",
        protocolBuffer: "PBVideoPlayer"
    }
};

/* eslint-disable */
function createBasePBVisibilityComponent() {
    return { visible: undefined };
}
/**
 * @public
 */
/**
 * @internal
 */
const PBVisibilityComponent = {
    encode(message, writer = _m0.Writer.create()) {
        if (message.visible !== undefined) {
            writer.uint32(8).bool(message.visible);
        }
        return writer;
    },
    decode(input, length) {
        const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
        let end = length === undefined ? reader.len : reader.pos + length;
        const message = createBasePBVisibilityComponent();
        while (reader.pos < end) {
            const tag = reader.uint32();
            switch (tag >>> 3) {
                case 1:
                    message.visible = reader.bool();
                    break;
                default:
                    reader.skipType(tag & 7);
                    break;
            }
        }
        return message;
    },
};

/**
 * @internal
 */
const VisibilityComponentSchema = {
    COMPONENT_ID: 1081,
    serialize(value, builder) {
        const writer = PBVisibilityComponent.encode(value);
        const buffer = new Uint8Array(writer.finish(), 0, writer.len);
        builder.writeBuffer(buffer, false);
    },
    deserialize(reader) {
        return PBVisibilityComponent.decode(reader.buffer(), reader.remainingBytes());
    },
    create() {
        // TODO: this is a hack.
        return PBVisibilityComponent.decode(new Uint8Array());
    },
    jsonSchema: {
        type: "object",
        properties: {},
        serializationType: "protocol-buffer",
        protocolBuffer: "PBVisibilityComponent"
    }
};

/** @public */  const Animator$1 = engine => engine.defineComponentFromSchema("core::Animator", AnimatorSchema);
/** @public */  const AudioSource = engine => engine.defineComponentFromSchema("core::AudioSource", AudioSourceSchema);
/** @public */  const AudioStream = engine => engine.defineComponentFromSchema("core::AudioStream", AudioStreamSchema);
/** @public */  const AvatarAttach = engine => engine.defineComponentFromSchema("core::AvatarAttach", AvatarAttachSchema);
/** @public */  const AvatarModifierArea = engine => engine.defineComponentFromSchema("core::AvatarModifierArea", AvatarModifierAreaSchema);
/** @public */  const AvatarShape = engine => engine.defineComponentFromSchema("core::AvatarShape", AvatarShapeSchema);
/** @public */  const Billboard = engine => engine.defineComponentFromSchema("core::Billboard", BillboardSchema);
/** @public */  const CameraMode = engine => engine.defineComponentFromSchema("core::CameraMode", CameraModeSchema);
/** @public */  const CameraModeArea = engine => engine.defineComponentFromSchema("core::CameraModeArea", CameraModeAreaSchema);
/** @public */  const GltfContainer = engine => engine.defineComponentFromSchema("core::GltfContainer", GltfContainerSchema);
/** @public */  const Material$2 = engine => engine.defineComponentFromSchema("core::Material", MaterialSchema);
/** @public */  const MeshCollider$2 = engine => engine.defineComponentFromSchema("core::MeshCollider", MeshColliderSchema);
/** @public */  const MeshRenderer$2 = engine => engine.defineComponentFromSchema("core::MeshRenderer", MeshRendererSchema);
/** @public */  const NftShape = engine => engine.defineComponentFromSchema("core::NftShape", NftShapeSchema);
/** @public */  const PointerEvents$1 = engine => engine.defineComponentFromSchema("core::PointerEvents", PointerEventsSchema);
/** @public */  const PointerEventsResult = (engine) => engine.defineValueSetComponentFromSchema("core::PointerEventsResult", PointerEventsResultSchema, {
    timestampFunction: (t) => t.timestamp,
    maxElements: 100
});
/** @public */  const PointerLock = engine => engine.defineComponentFromSchema("core::PointerLock", PointerLockSchema);
/** @public */  const Raycast = engine => engine.defineComponentFromSchema("core::Raycast", RaycastSchema);
/** @public */  const RaycastResult = engine => engine.defineComponentFromSchema("core::RaycastResult", RaycastResultSchema);
/** @public */  const TextShape = engine => engine.defineComponentFromSchema("core::TextShape", TextShapeSchema);
/** @public */  const UiBackground = engine => engine.defineComponentFromSchema("core::UiBackground", UiBackgroundSchema);
/** @public */  const UiDropdown = engine => engine.defineComponentFromSchema("core::UiDropdown", UiDropdownSchema);
/** @public */  const UiDropdownResult = engine => engine.defineComponentFromSchema("core::UiDropdownResult", UiDropdownResultSchema);
/** @public */  const UiInput = engine => engine.defineComponentFromSchema("core::UiInput", UiInputSchema);
/** @public */  const UiInputResult = engine => engine.defineComponentFromSchema("core::UiInputResult", UiInputResultSchema);
/** @public */  const UiText = engine => engine.defineComponentFromSchema("core::UiText", UiTextSchema);
/** @public */  const UiTransform = engine => engine.defineComponentFromSchema("core::UiTransform", UiTransformSchema);
/** @public */  const VideoPlayer = engine => engine.defineComponentFromSchema("core::VideoPlayer", VideoPlayerSchema);
/** @public */  const VisibilityComponent = engine => engine.defineComponentFromSchema("core::VisibilityComponent", VisibilityComponentSchema);

function defineAnimatorComponent(engine) {
    const theComponent = Animator$1(engine);
    /**
     * @returns The tuple [animator, clip]
     */
    function getClipAndAnimator(entity, name) {
        const anim = theComponent.getMutableOrNull(entity);
        if (!anim)
            return [null, null];
        const state = anim.states.find((item) => item.name === name || item.clip === name);
        if (!state)
            return [anim, null];
        return [anim, state];
    }
    return {
        ...theComponent,
        getClipOrNull(entity, name) {
            const [_, state] = getClipAndAnimator(entity, name);
            return state;
        },
        getClip(entity, name) {
            const [animator, state] = getClipAndAnimator(entity, name);
            if (!animator) {
                throw new Error(`There is no Animator found in the entity ${entity}`);
            }
            if (!state) {
                throw new Error(`The Animator component of ${entity} has no the state ${name}`);
            }
            return state;
        },
        playSingleAnimation(entity, name, shouldReset = true) {
            const [animator, state] = getClipAndAnimator(entity, name);
            if (!animator || !state)
                return false;
            // Reset all other animations
            for (const state of animator.states) {
                state.playing = false;
                state.shouldReset = true;
            }
            state.playing = true;
            state.shouldReset = shouldReset;
            return true;
        },
        stopAllAnimations(entity, resetCursor = true) {
            // Get the mutable to modifying
            const animator = theComponent.getMutableOrNull(entity);
            if (!animator)
                return false;
            // Reset all other animations
            for (const state of animator.states) {
                state.playing = false;
                state.shouldReset = resetCursor;
            }
            return true;
        }
    };
}

const TextureHelper = {
    Common(texture) {
        return {
            tex: {
                $case: 'texture',
                texture
            }
        };
    },
    Avatar(avatarTexture) {
        return {
            tex: {
                $case: 'avatarTexture',
                avatarTexture
            }
        };
    },
    Video(videoTexture) {
        return {
            tex: {
                $case: 'videoTexture',
                videoTexture
            }
        };
    }
};
function defineMaterialComponent(engine) {
    const theComponent = Material$2(engine);
    return {
        ...theComponent,
        Texture: TextureHelper,
        setBasicMaterial(entity, material) {
            theComponent.createOrReplace(entity, {
                material: {
                    $case: 'unlit',
                    unlit: material
                }
            });
        },
        setPbrMaterial(entity, material) {
            theComponent.createOrReplace(entity, {
                material: {
                    $case: 'pbr',
                    pbr: material
                }
            });
        }
    };
}

function defineMeshColliderComponent(engine) {
    const theComponent = MeshCollider$2(engine);
    function getCollisionMask(layers) {
        if (Array.isArray(layers)) {
            return layers.map((item) => item).reduce((prev, item) => prev | item, 0);
        }
        else if (layers) {
            return layers;
        }
    }
    return {
        ...theComponent,
        setBox(entity, colliderLayers) {
            theComponent.createOrReplace(entity, {
                mesh: { $case: 'box', box: {} },
                collisionMask: getCollisionMask(colliderLayers)
            });
        },
        setPlane(entity, colliderLayers) {
            theComponent.createOrReplace(entity, {
                mesh: { $case: 'plane', plane: {} },
                collisionMask: getCollisionMask(colliderLayers)
            });
        },
        setCylinder(entity, radiusBottom, radiusTop, colliderLayers) {
            theComponent.createOrReplace(entity, {
                mesh: { $case: 'cylinder', cylinder: { radiusBottom, radiusTop } },
                collisionMask: getCollisionMask(colliderLayers)
            });
        },
        setSphere(entity, colliderLayers) {
            theComponent.createOrReplace(entity, {
                mesh: { $case: 'sphere', sphere: {} },
                collisionMask: getCollisionMask(colliderLayers)
            });
        }
    };
}

function defineMeshRendererComponent(engine) {
    const theComponent = MeshRenderer$2(engine);
    return {
        ...theComponent,
        setBox(entity, uvs) {
            theComponent.createOrReplace(entity, {
                mesh: { $case: 'box', box: { uvs: uvs || [] } }
            });
        },
        setPlane(entity, uvs) {
            theComponent.createOrReplace(entity, {
                mesh: { $case: 'plane', plane: { uvs: uvs || [] } }
            });
        },
        setCylinder(entity, radiusBottom, radiusTop) {
            theComponent.createOrReplace(entity, {
                mesh: { $case: 'cylinder', cylinder: { radiusBottom, radiusTop } }
            });
        },
        setSphere(entity) {
            theComponent.createOrReplace(entity, {
                mesh: { $case: 'sphere', sphere: {} }
            });
        }
    };
}

/**
 * @internal
 */
/** @internal */
const TRANSFORM_LENGTH = 44;
/** @internal */
const TransformSchema = {
    serialize(value, builder) {
        const ptr = builder.incrementWriteOffset(TRANSFORM_LENGTH);
        builder.setFloat32(ptr, value.position.x);
        builder.setFloat32(ptr + 4, value.position.y);
        builder.setFloat32(ptr + 8, value.position.z);
        builder.setFloat32(ptr + 12, value.rotation.x);
        builder.setFloat32(ptr + 16, value.rotation.y);
        builder.setFloat32(ptr + 20, value.rotation.z);
        builder.setFloat32(ptr + 24, value.rotation.w);
        builder.setFloat32(ptr + 28, value.scale.x);
        builder.setFloat32(ptr + 32, value.scale.y);
        builder.setFloat32(ptr + 36, value.scale.z);
        builder.setUint32(ptr + 40, value.parent || 0);
    },
    deserialize(reader) {
        const ptr = reader.incrementReadOffset(TRANSFORM_LENGTH);
        return {
            position: {
                x: reader.getFloat32(ptr),
                y: reader.getFloat32(ptr + 4),
                z: reader.getFloat32(ptr + 8)
            },
            rotation: {
                x: reader.getFloat32(ptr + 12),
                y: reader.getFloat32(ptr + 16),
                z: reader.getFloat32(ptr + 20),
                w: reader.getFloat32(ptr + 24)
            },
            scale: {
                x: reader.getFloat32(ptr + 28),
                y: reader.getFloat32(ptr + 32),
                z: reader.getFloat32(ptr + 36)
            },
            parent: reader.getUint32(ptr + 40)
        };
    },
    create() {
        return {
            position: { x: 0, y: 0, z: 0 },
            scale: { x: 1, y: 1, z: 1 },
            rotation: { x: 0, y: 0, z: 0, w: 1 },
            parent: 0
        };
    },
    extend(value) {
        return {
            position: { x: 0, y: 0, z: 0 },
            scale: { x: 1, y: 1, z: 1 },
            rotation: { x: 0, y: 0, z: 0, w: 1 },
            parent: 0,
            ...value
        };
    },
    jsonSchema: {
        type: 'object',
        properties: {
            position: {
                type: 'object',
                properties: {
                    x: { type: 'number' },
                    y: { type: 'number' },
                    z: { type: 'number' }
                }
            },
            scale: {
                type: 'object',
                properties: {
                    x: { type: 'number' },
                    y: { type: 'number' },
                    z: { type: 'number' }
                }
            },
            rotation: {
                type: 'object',
                properties: {
                    x: { type: 'number' },
                    y: { type: 'number' },
                    z: { type: 'number' },
                    w: { type: 'number' }
                }
            },
            parent: { type: 'integer' }
        },
        serializationType: 'transform'
    }
};
function defineTransformComponent(engine) {
    const transformDef = engine.defineComponentFromSchema('core::Transform', TransformSchema);
    return {
        ...transformDef,
        create(entity, val) {
            return transformDef.create(entity, val);
        },
        createOrReplace(entity, val) {
            return transformDef.createOrReplace(entity, val);
        }
    };
}

const Transform$1 = (engine) => defineTransformComponent(engine);

const Material$1 = (engine) => defineMaterialComponent(engine);

const Animator = (engine) => defineAnimatorComponent(engine);

const MeshRenderer$1 = (engine) => defineMeshRendererComponent(engine);

const MeshCollider$1 = (engine) => defineMeshColliderComponent(engine);

/**
 * Autogenerated mapping of core components to their component numbers
 */
const coreComponentMappings = {
    "core::Transform": 1,
    "core::Animator": 1042,
    "core::AudioSource": 1020,
    "core::AudioStream": 1021,
    "core::AvatarAttach": 1073,
    "core::AvatarModifierArea": 1070,
    "core::AvatarShape": 1080,
    "core::Billboard": 1090,
    "core::CameraMode": 1072,
    "core::CameraModeArea": 1071,
    "core::GltfContainer": 1041,
    "core::Material": 1017,
    "core::MeshCollider": 1019,
    "core::MeshRenderer": 1018,
    "core::NftShape": 1040,
    "core::PointerEvents": 1062,
    "core::PointerEventsResult": 1063,
    "core::PointerLock": 1074,
    "core::Raycast": 1067,
    "core::RaycastResult": 1068,
    "core::TextShape": 1030,
    "core::UiBackground": 1053,
    "core::UiDropdown": 1094,
    "core::UiDropdownResult": 1096,
    "core::UiInput": 1093,
    "core::UiInputResult": 1095,
    "core::UiText": 1052,
    "core::UiTransform": 1050,
    "core::VideoPlayer": 1043,
    "core::VisibilityComponent": 1081
};

var utf8Exports = requireUtf8();

const CRC_TABLE = new Int32Array([
    0x00000000, 0x77073096, 0xee0e612c, 0x990951ba, 0x076dc419, 0x706af48f, 0xe963a535, 0x9e6495a3, 0x0edb8832,
    0x79dcb8a4, 0xe0d5e91e, 0x97d2d988, 0x09b64c2b, 0x7eb17cbd, 0xe7b82d07, 0x90bf1d91, 0x1db71064, 0x6ab020f2,
    0xf3b97148, 0x84be41de, 0x1adad47d, 0x6ddde4eb, 0xf4d4b551, 0x83d385c7, 0x136c9856, 0x646ba8c0, 0xfd62f97a,
    0x8a65c9ec, 0x14015c4f, 0x63066cd9, 0xfa0f3d63, 0x8d080df5, 0x3b6e20c8, 0x4c69105e, 0xd56041e4, 0xa2677172,
    0x3c03e4d1, 0x4b04d447, 0xd20d85fd, 0xa50ab56b, 0x35b5a8fa, 0x42b2986c, 0xdbbbc9d6, 0xacbcf940, 0x32d86ce3,
    0x45df5c75, 0xdcd60dcf, 0xabd13d59, 0x26d930ac, 0x51de003a, 0xc8d75180, 0xbfd06116, 0x21b4f4b5, 0x56b3c423,
    0xcfba9599, 0xb8bda50f, 0x2802b89e, 0x5f058808, 0xc60cd9b2, 0xb10be924, 0x2f6f7c87, 0x58684c11, 0xc1611dab,
    0xb6662d3d, 0x76dc4190, 0x01db7106, 0x98d220bc, 0xefd5102a, 0x71b18589, 0x06b6b51f, 0x9fbfe4a5, 0xe8b8d433,
    0x7807c9a2, 0x0f00f934, 0x9609a88e, 0xe10e9818, 0x7f6a0dbb, 0x086d3d2d, 0x91646c97, 0xe6635c01, 0x6b6b51f4,
    0x1c6c6162, 0x856530d8, 0xf262004e, 0x6c0695ed, 0x1b01a57b, 0x8208f4c1, 0xf50fc457, 0x65b0d9c6, 0x12b7e950,
    0x8bbeb8ea, 0xfcb9887c, 0x62dd1ddf, 0x15da2d49, 0x8cd37cf3, 0xfbd44c65, 0x4db26158, 0x3ab551ce, 0xa3bc0074,
    0xd4bb30e2, 0x4adfa541, 0x3dd895d7, 0xa4d1c46d, 0xd3d6f4fb, 0x4369e96a, 0x346ed9fc, 0xad678846, 0xda60b8d0,
    0x44042d73, 0x33031de5, 0xaa0a4c5f, 0xdd0d7cc9, 0x5005713c, 0x270241aa, 0xbe0b1010, 0xc90c2086, 0x5768b525,
    0x206f85b3, 0xb966d409, 0xce61e49f, 0x5edef90e, 0x29d9c998, 0xb0d09822, 0xc7d7a8b4, 0x59b33d17, 0x2eb40d81,
    0xb7bd5c3b, 0xc0ba6cad, 0xedb88320, 0x9abfb3b6, 0x03b6e20c, 0x74b1d29a, 0xead54739, 0x9dd277af, 0x04db2615,
    0x73dc1683, 0xe3630b12, 0x94643b84, 0x0d6d6a3e, 0x7a6a5aa8, 0xe40ecf0b, 0x9309ff9d, 0x0a00ae27, 0x7d079eb1,
    0xf00f9344, 0x8708a3d2, 0x1e01f268, 0x6906c2fe, 0xf762575d, 0x806567cb, 0x196c3671, 0x6e6b06e7, 0xfed41b76,
    0x89d32be0, 0x10da7a5a, 0x67dd4acc, 0xf9b9df6f, 0x8ebeeff9, 0x17b7be43, 0x60b08ed5, 0xd6d6a3e8, 0xa1d1937e,
    0x38d8c2c4, 0x4fdff252, 0xd1bb67f1, 0xa6bc5767, 0x3fb506dd, 0x48b2364b, 0xd80d2bda, 0xaf0a1b4c, 0x36034af6,
    0x41047a60, 0xdf60efc3, 0xa867df55, 0x316e8eef, 0x4669be79, 0xcb61b38c, 0xbc66831a, 0x256fd2a0, 0x5268e236,
    0xcc0c7795, 0xbb0b4703, 0x220216b9, 0x5505262f, 0xc5ba3bbe, 0xb2bd0b28, 0x2bb45a92, 0x5cb36a04, 0xc2d7ffa7,
    0xb5d0cf31, 0x2cd99e8b, 0x5bdeae1d, 0x9b64c2b0, 0xec63f226, 0x756aa39c, 0x026d930a, 0x9c0906a9, 0xeb0e363f,
    0x72076785, 0x05005713, 0x95bf4a82, 0xe2b87a14, 0x7bb12bae, 0x0cb61b38, 0x92d28e9b, 0xe5d5be0d, 0x7cdcefb7,
    0x0bdbdf21, 0x86d3d2d4, 0xf1d4e242, 0x68ddb3f8, 0x1fda836e, 0x81be16cd, 0xf6b9265b, 0x6fb077e1, 0x18b74777,
    0x88085ae6, 0xff0f6a70, 0x66063bca, 0x11010b5c, 0x8f659eff, 0xf862ae69, 0x616bffd3, 0x166ccf45, 0xa00ae278,
    0xd70dd2ee, 0x4e048354, 0x3903b3c2, 0xa7672661, 0xd06016f7, 0x4969474d, 0x3e6e77db, 0xaed16a4a, 0xd9d65adc,
    0x40df0b66, 0x37d83bf0, 0xa9bcae53, 0xdebb9ec5, 0x47b2cf7f, 0x30b5ffe9, 0xbdbdf21c, 0xcabac28a, 0x53b39330,
    0x24b4a3a6, 0xbad03605, 0xcdd70693, 0x54de5729, 0x23d967bf, 0xb3667a2e, 0xc4614ab8, 0x5d681b02, 0x2a6f2b94,
    0xb40bbe37, 0xc30c8ea1, 0x5a05df1b, 0x2d02ef8d
]);
function _crc32(buf, previous) {
    let crc = ~~previous ^ -1;
    for (let n = 0; n < buf.length; n++) {
        crc = CRC_TABLE[(crc ^ buf[n]) & 0xff] ^ (crc >>> 8);
    }
    return crc ^ -1;
}
function unsignedCRC32(data, prev = 0) {
    return _crc32(data, prev) >>> 0;
}

// Max possible pre-defined (static) component.
const MAX_STATIC_COMPONENT = 1 << 11; // 2048
/**
 * All components that are not part of the coreComponentMappings MUST yield
 * a componentNumber (componentId) greather than MAX_STATIC_COMPONENT.
 * For that reason, we simply add MAX_STATIC_COMPONENT and trim to the domain 2^32
 */
function componentNumberFromName(componentName) {
    if (coreComponentMappings[componentName])
        return coreComponentMappings[componentName];
    const bytes = new Uint8Array(128);
    utf8Exports.write(componentName, bytes, 0);
    return ((unsignedCRC32(bytes) + MAX_STATIC_COMPONENT) & 4294967295) >>> 0;
}

/* istanbul ignore file */
function checkNotThenable(t, error) {
    {
        if (t && typeof t === 'object' && typeof t.then === 'function') {
            throw new Error(error);
        }
    }
    return t;
}

/**
 * @internal
 */
const IArray = (type) => {
    return {
        serialize(value, builder) {
            builder.writeUint32(value.length);
            for (const item of value) {
                type.serialize(item, builder);
            }
        },
        deserialize(reader) {
            const newArray = [];
            const length = reader.readUint32();
            for (let index = 0; index < length; index++) {
                newArray.push(type.deserialize(reader));
            }
            return newArray;
        },
        create() {
            return [];
        },
        jsonSchema: {
            type: 'array',
            items: type.jsonSchema,
            serializationType: 'array'
        }
    };
};

/**
 * @internal
 */
const Bool = {
    serialize(value, builder) {
        builder.writeInt8(value ? 1 : 0);
    },
    deserialize(reader) {
        return reader.readInt8() === 1;
    },
    create() {
        return false;
    },
    jsonSchema: {
        type: 'boolean',
        serializationType: 'boolean'
    }
};

/**
 * @internal
 */
const Int64 = {
    serialize(value, builder) {
        builder.writeInt64(BigInt(value));
    },
    deserialize(reader) {
        return Number(reader.readInt64());
    },
    create() {
        return 0;
    },
    jsonSchema: {
        type: 'integer',
        serializationType: 'int64'
    }
};
/**
 * @internal
 */
const Int32 = {
    serialize(value, builder) {
        builder.writeInt32(value);
    },
    deserialize(reader) {
        return reader.readInt32();
    },
    create() {
        return 0;
    },
    jsonSchema: {
        type: 'integer',
        serializationType: 'int32'
    }
};
/**
 * @public
 */
const Int16 = {
    serialize(value, builder) {
        builder.writeInt16(value);
    },
    deserialize(reader) {
        return reader.readInt16();
    },
    create() {
        return 0;
    },
    jsonSchema: {
        type: 'integer',
        serializationType: 'int16'
    }
};
/**
 * @public
 */
const Int8 = {
    serialize(value, builder) {
        builder.writeInt8(value);
    },
    deserialize(reader) {
        return reader.readInt8();
    },
    create() {
        return 0;
    },
    jsonSchema: {
        type: 'integer',
        serializationType: 'int8'
    }
};

/**
 * @internal
 */
const FlatString = {
    serialize(value, builder) {
        builder.writeUtf8String(value);
    },
    deserialize(reader) {
        return reader.readUtf8String();
    },
    create() {
        return '';
    },
    jsonSchema: {
        type: 'string',
        serializationType: 'utf8-string'
    }
};
/**
 * @internal
 */
const EcsString = FlatString;

/**
 * Validates the enum to ensure all member values are numbers and within the range of Int32.
 * @param enumValue The enum to be checked.
 * @throws If any member value is not a number or is outside the range of Int32.
 */
function validateMemberValuesAreNumbersAndInRangeInt32(enumValue) {
    const MIN_VALUE = -(2 ** 31), MAX_VALUE = 2 ** 31 - 1;
    let valueCount = 0, totalCount = 0;
    for (const key in enumValue) {
        if (typeof enumValue[key] === 'number') {
            if (enumValue[key] > MAX_VALUE || enumValue[key] < MIN_VALUE) {
                throw new Error(`Enum member values must be numbers within the range of ${MIN_VALUE} to ${MAX_VALUE}.`);
            }
            valueCount++;
        }
        totalCount++;
    }
    if (totalCount !== valueCount * 2) {
        throw new Error('All enum member values must be of numeric type.');
    }
}
/**
 * Validates the enum to ensure all member values are of string type.
 * @param enumValue The enum to be checked.
 * @throws If any member value is not of string type.
 */
function validateMemberValuesAreStrings(enumValue) {
    for (const key in enumValue) {
        if (typeof enumValue[key] !== 'string') {
            throw new Error('All enum member values must be of string type.');
        }
    }
}
/**
 * @internal
 */
const IntEnumReflectionType = 'enum-int';
/**
 * @internal
 */
const IntEnum = (enumObject, defaultValue) => {
    validateMemberValuesAreNumbersAndInRangeInt32(enumObject);
    return {
        serialize(value, builder) {
            Int32.serialize(value, builder);
        },
        deserialize(reader) {
            return Int32.deserialize(reader);
        },
        create() {
            return defaultValue;
        },
        jsonSchema: {
            // JSON-schema
            type: 'integer',
            enum: Object.values(enumObject).filter((item) => Number.isInteger(item)),
            default: defaultValue,
            // @dcl/ecs Schema Spec
            serializationType: IntEnumReflectionType,
            enumObject
        }
    };
};
/**
 * @internal
 */
const StringEnumReflectionType = 'enum-string';
/**
 * @internal
 */
const StringEnum = (enumObject, defaultValue) => {
    validateMemberValuesAreStrings(enumObject);
    // String enum has the exact mapping from key (our reference in code) to values
    return {
        serialize(value, builder) {
            FlatString.serialize(value, builder);
        },
        deserialize(reader) {
            return FlatString.deserialize(reader);
        },
        create() {
            return defaultValue;
        },
        jsonSchema: {
            // JSON-schema
            type: 'string',
            enum: Object.values(enumObject),
            default: defaultValue,
            // @dcl/ecs Schema Spec
            serializationType: StringEnumReflectionType,
            enumObject
        }
    };
};

/**
 * @internal
 */
const Float32 = {
    serialize(value, builder) {
        builder.writeFloat32(value);
    },
    deserialize(reader) {
        return reader.readFloat32();
    },
    create() {
        return 0.0;
    },
    jsonSchema: {
        type: 'number',
        serializationType: 'float32'
    }
};
/**
 * @internal
 */
const Float64 = {
    serialize(value, builder) {
        builder.writeFloat64(value);
    },
    deserialize(reader) {
        return reader.readFloat64();
    },
    create() {
        return 0.0;
    },
    jsonSchema: {
        type: 'number',
        serializationType: 'float64'
    }
};

/**
 * @internal
 */
const Color3Schema = {
    serialize(value, builder) {
        builder.writeFloat32(value.r);
        builder.writeFloat32(value.g);
        builder.writeFloat32(value.b);
    },
    deserialize(reader) {
        return {
            r: reader.readFloat32(),
            g: reader.readFloat32(),
            b: reader.readFloat32()
        };
    },
    create() {
        return { r: 0, g: 0, b: 0 };
    },
    jsonSchema: {
        type: 'object',
        properties: {
            r: { type: 'number' },
            g: { type: 'number' },
            b: { type: 'number' }
        },
        serializationType: 'color3'
    }
};

/**
 * @internal
 */
const Color4Schema = {
    serialize(value, builder) {
        builder.writeFloat32(value.r);
        builder.writeFloat32(value.g);
        builder.writeFloat32(value.b);
        builder.writeFloat32(value.a);
    },
    deserialize(reader) {
        return {
            r: reader.readFloat32(),
            g: reader.readFloat32(),
            b: reader.readFloat32(),
            a: reader.readFloat32()
        };
    },
    create() {
        return { r: 0, g: 0, b: 0, a: 0 };
    },
    jsonSchema: {
        type: 'object',
        properties: {
            r: { type: 'number' },
            g: { type: 'number' },
            b: { type: 'number' },
            a: { type: 'number' }
        },
        serializationType: 'color4'
    }
};

/**
 * @internal
 */
const EntitySchema = {
    serialize(value, builder) {
        builder.writeInt32(value);
    },
    deserialize(reader) {
        return reader.readInt32();
    },
    create() {
        return 0;
    },
    jsonSchema: {
        type: 'integer',
        serializationType: 'entity'
    }
};

/**
 * @internal
 */
const QuaternionSchema = {
    serialize(value, builder) {
        builder.writeFloat32(value.x);
        builder.writeFloat32(value.y);
        builder.writeFloat32(value.z);
        builder.writeFloat32(value.w);
    },
    deserialize(reader) {
        return {
            x: reader.readFloat32(),
            y: reader.readFloat32(),
            z: reader.readFloat32(),
            w: reader.readFloat32()
        };
    },
    create() {
        return { x: 0, y: 0, z: 0, w: 0 };
    },
    jsonSchema: {
        type: 'object',
        properties: {
            x: { type: 'number' },
            y: { type: 'number' },
            z: { type: 'number' },
            w: { type: 'number' }
        },
        serializationType: 'quaternion'
    }
};

/**
 * @internal
 */
const Vector3Schema = {
    serialize(value, builder) {
        builder.writeFloat32(value.x);
        builder.writeFloat32(value.y);
        builder.writeFloat32(value.z);
    },
    deserialize(reader) {
        return {
            x: reader.readFloat32(),
            y: reader.readFloat32(),
            z: reader.readFloat32()
        };
    },
    create() {
        return { x: 0, y: 0, z: 0 };
    },
    jsonSchema: {
        type: 'object',
        properties: {
            x: { type: 'number' },
            y: { type: 'number' },
            z: { type: 'number' },
            w: { type: 'number' }
        },
        serializationType: 'vector3'
    }
};

/**
 * @internal
 */
const IMap = (spec, defaultValue) => {
    const specReflection = Object.keys(spec).reduce((specReflection, currentKey) => {
        specReflection[currentKey] = spec[currentKey].jsonSchema;
        return specReflection;
    }, {});
    return {
        serialize(value, builder) {
            for (const key in spec) {
                spec[key].serialize(value[key], builder);
            }
        },
        deserialize(reader) {
            const newValue = {};
            for (const key in spec) {
                newValue[key] = spec[key].deserialize(reader);
            }
            return newValue;
        },
        create() {
            const newValue = {};
            for (const key in spec) {
                newValue[key] = spec[key].create();
            }
            return { ...newValue, ...defaultValue };
        },
        extend: (base) => {
            const newValue = {};
            for (const key in spec) {
                newValue[key] = spec[key].create();
            }
            return { ...newValue, ...defaultValue, ...base };
        },
        jsonSchema: {
            type: 'object',
            properties: specReflection,
            serializationType: 'map'
        }
    };
};

/**
 * @internal
 */
const IOptional = (spec) => {
    return {
        serialize(value, builder) {
            if (value) {
                builder.writeInt8(1);
                spec.serialize(value, builder);
            }
            else {
                builder.writeInt8(0);
            }
        },
        deserialize(reader) {
            const exists = reader.readInt8();
            if (exists) {
                return spec.deserialize(reader);
            }
        },
        create() {
            return undefined;
        },
        jsonSchema: {
            type: spec.jsonSchema.type,
            serializationType: 'optional',
            optionalJsonSchema: spec.jsonSchema
        }
    };
};

/**
 * @public
 */
var Schemas;
(function (Schemas) {
    /** @public */
    Schemas.Boolean = Bool;
    /** @public */
    Schemas.String = EcsString;
    /** @public */
    Schemas.Float = Float32;
    /** @public */
    Schemas.Double = Float64;
    /** @public */
    Schemas.Byte = Int8;
    /** @public */
    Schemas.Short = Int16;
    /** @public */
    Schemas.Int = Int32;
    /** @public */
    Schemas.Int64 = Int64;
    /** @public */
    Schemas.Number = Float32;
    /** @public */
    Schemas.Vector3 = Vector3Schema;
    /** @public */
    Schemas.Quaternion = QuaternionSchema;
    /** @public */
    Schemas.Color3 = Color3Schema;
    /** @public */
    Schemas.Color4 = Color4Schema;
    /** @public */
    Schemas.Entity = EntitySchema;
    /** @public */
    Schemas.EnumNumber = IntEnum;
    /** @public */
    Schemas.EnumString = StringEnum;
    /** @public */
    Schemas.Array = IArray;
    /** @public */
    Schemas.Map = IMap;
    /** @public */
    Schemas.Optional = IOptional;
})(Schemas || (Schemas = {}));

/**
 *
 * @returns a new GSet
 */
function createVersionGSet() {
    const lastVersion = new Map();
    return {
        /**
         *
         * @param number
         * @param version
         * @returns
         */
        addTo(number, version) {
            /* istanbul ignore next */
            if (version < 0) {
                /* istanbul ignore next */
                return false;
            }
            const currentValue = lastVersion.get(number);
            // If the version is >=, it means the value it's already in the set
            if (currentValue !== undefined && currentValue >= version) {
                return true;
            }
            lastVersion.set(number, version);
            return true;
        },
        /**
         * @returns the set with [number, version] of each value
         */
        has(n, v) {
            const currentValue = lastVersion.get(n);
            // If the version is >=, it means the value it's already in the set
            if (currentValue !== undefined && currentValue >= v) {
                return true;
            }
            return false;
        },
        /**
         * Warning: this function returns the reference to the internal map,
         *  if you need to mutate some value, make a copy.
         * For optimization purpose the copy isn't made here.
         *
         * @returns the map of number to version
         */
        getMap() {
            return lastVersion;
        }
    };
}

/**
 * @internal
 */
const MAX_U16 = 0xffff;
const MASK_UPPER_16_ON_32 = 0xffff0000;
// This type matches with @dcl/crdt entity type.
/**
 * @internal
 * This first 512 entities are reserved by the renderer
 */
const RESERVED_STATIC_ENTITIES = 512;
/**
 * @internal
 */
const MAX_ENTITY_NUMBER = MAX_U16;
/**
 * @internal
 */
var EntityUtils;
(function (EntityUtils) {
    /**
     * @returns [entityNumber, entityVersion]
     */
    function fromEntityId(entityId) {
        return [(entityId & MAX_U16) >>> 0, (((entityId & MASK_UPPER_16_ON_32) >> 16) & MAX_U16) >>> 0];
    }
    EntityUtils.fromEntityId = fromEntityId;
    /**
     * @returns compound number from entityNumber and entityVerison
     */
    function toEntityId(entityNumber, entityVersion) {
        return (((entityNumber & MAX_U16) | ((entityVersion & MAX_U16) << 16)) >>> 0);
    }
    EntityUtils.toEntityId = toEntityId;
})(EntityUtils || (EntityUtils = {}));
/**
 * @public
 */
var EntityState;
(function (EntityState) {
    EntityState[EntityState["Unknown"] = 0] = "Unknown";
    /**
     * The entity was generated and added to the usedEntities set
     */
    EntityState[EntityState["UsedEntity"] = 1] = "UsedEntity";
    /**
     * The entity was removed from current engine or remotely
     */
    EntityState[EntityState["Removed"] = 2] = "Removed";
    /**
     * The entity is reserved number.
     */
    EntityState[EntityState["Reserved"] = 3] = "Reserved";
})(EntityState || (EntityState = {}));
/**
 * @internal
 */
function EntityContainer() {
    let entityCounter = RESERVED_STATIC_ENTITIES;
    const usedEntities = new Set();
    let toRemoveEntities = [];
    const removedEntities = createVersionGSet();
    function generateNewEntity() {
        if (entityCounter > MAX_ENTITY_NUMBER - 1) {
            throw new Error(`It fails trying to generate an entity out of range ${MAX_ENTITY_NUMBER}.`);
        }
        const entityNumber = entityCounter++;
        const entityVersion = removedEntities.getMap().has(entityNumber)
            ? removedEntities.getMap().get(entityNumber) + 1
            : 0;
        const entity = EntityUtils.toEntityId(entityNumber, entityVersion);
        usedEntities.add(entity);
        return entity;
    }
    function generateEntity() {
        // If all entities until `entityCounter` are being used, we need to generate another one
        if (usedEntities.size + RESERVED_STATIC_ENTITIES >= entityCounter) {
            return generateNewEntity();
        }
        for (const [number, version] of removedEntities.getMap()) {
            if (version < MAX_U16) {
                const entity = EntityUtils.toEntityId(number, version + 1);
                // If the entity is not being used, we can re-use it
                // If the entity was removed in this tick, we're not counting for the usedEntities, but we have it in the toRemoveEntityArray
                if (!usedEntities.has(entity) && !toRemoveEntities.includes(entity)) {
                    usedEntities.add(entity);
                    return entity;
                }
            }
        }
        return generateNewEntity();
    }
    function removeEntity(entity) {
        if (entity < RESERVED_STATIC_ENTITIES)
            return false;
        if (usedEntities.has(entity)) {
            usedEntities.delete(entity);
            toRemoveEntities.push(entity);
        }
        else {
            updateRemovedEntity(entity);
        }
        return true;
    }
    function releaseRemovedEntities() {
        const arr = toRemoveEntities;
        if (arr.length) {
            toRemoveEntities = [];
            for (const entity of arr) {
                const [n, v] = EntityUtils.fromEntityId(entity);
                removedEntities.addTo(n, v);
            }
        }
        return arr;
    }
    function updateRemovedEntity(entity) {
        const [n, v] = EntityUtils.fromEntityId(entity);
        // Update the removed entities map
        removedEntities.addTo(n, v);
        // Remove the usedEntities if exist
        for (let i = 0; i <= v; i++) {
            usedEntities.delete(EntityUtils.toEntityId(n, i));
        }
        return true;
    }
    function updateUsedEntity(entity) {
        const [n, v] = EntityUtils.fromEntityId(entity);
        // if the entity was removed then abort fast
        if (removedEntities.has(n, v))
            return false;
        // Update
        if (v > 0) {
            for (let i = 0; i <= v - 1; i++) {
                usedEntities.delete(EntityUtils.toEntityId(n, i));
            }
            removedEntities.addTo(n, v - 1);
        }
        usedEntities.add(entity);
        return true;
    }
    function getEntityState(entity) {
        const [n, v] = EntityUtils.fromEntityId(entity);
        if (n < RESERVED_STATIC_ENTITIES) {
            return EntityState.Reserved;
        }
        if (usedEntities.has(entity)) {
            return EntityState.UsedEntity;
        }
        const removedVersion = removedEntities.getMap().get(n);
        if (removedVersion !== undefined && removedVersion >= v) {
            return EntityState.Removed;
        }
        return EntityState.Unknown;
    }
    return {
        generateEntity,
        removeEntity,
        getExistingEntities() {
            return new Set(usedEntities);
        },
        getEntityState,
        releaseRemovedEntities,
        updateRemovedEntity,
        updateUsedEntity
    };
}

var __classPrivateFieldGet = (undefined && undefined.__classPrivateFieldGet) || function (receiver, state, kind, f) {
    if (kind === "a" && !f) throw new TypeError("Private accessor was defined without a getter");
    if (typeof state === "function" ? receiver !== state || !f : !state.has(receiver)) throw new TypeError("Cannot read private member from an object whose class did not declare it");
    return kind === "m" ? f : kind === "a" ? f.call(receiver) : f ? f.value : state.get(receiver);
};
var _ReadWriteByteBuffer_instances, _ReadWriteByteBuffer_woAdd, _ReadWriteByteBuffer_roAdd;
/**
 * Take the max between currentSize and intendedSize and then plus 1024. Then,
 *  find the next nearer multiple of 1024.
 * @param currentSize - number
 * @param intendedSize - number
 * @returns the calculated number
 */
function getNextSize(currentSize, intendedSize) {
    const minNewSize = Math.max(currentSize, intendedSize) + 1024;
    return Math.ceil(minNewSize / 1024) * 1024;
}
const defaultInitialCapacity = 10240;
/**
 * ByteBuffer is a wrapper of DataView which also adds a read and write offset.
 *  Also in a write operation it resizes the buffer is being used if it needs.
 *
 * - Use read and write function to generate or consume data.
 * - Use set and get only if you are sure that you're doing.
 */
class ReadWriteByteBuffer {
    /**
     * @param buffer - The initial buffer, provide a buffer if you need to set "initial capacity"
     * @param readingOffset - Set the cursor where begins to read. Default 0
     * @param writingOffset - Set the cursor to not start writing from the begin of it. Defaults to the buffer size
     */
    constructor(buffer, readingOffset, writingOffset) {
        _ReadWriteByteBuffer_instances.add(this);
        this._buffer = buffer || new Uint8Array(defaultInitialCapacity);
        this.view = new DataView(this._buffer.buffer, this._buffer.byteOffset);
        this.woffset = writingOffset ?? (buffer ? this._buffer.length : null) ?? 0;
        this.roffset = readingOffset ?? 0;
    }
    buffer() {
        return this._buffer;
    }
    bufferLength() {
        return this._buffer.length;
    }
    resetBuffer() {
        this.roffset = 0;
        this.woffset = 0;
    }
    currentReadOffset() {
        return this.roffset;
    }
    currentWriteOffset() {
        return this.woffset;
    }
    incrementReadOffset(amount) {
        return __classPrivateFieldGet(this, _ReadWriteByteBuffer_instances, "m", _ReadWriteByteBuffer_roAdd).call(this, amount);
    }
    remainingBytes() {
        return this.woffset - this.roffset;
    }
    readFloat32() {
        return this.view.getFloat32(__classPrivateFieldGet(this, _ReadWriteByteBuffer_instances, "m", _ReadWriteByteBuffer_roAdd).call(this, 4), true);
    }
    readFloat64() {
        return this.view.getFloat64(__classPrivateFieldGet(this, _ReadWriteByteBuffer_instances, "m", _ReadWriteByteBuffer_roAdd).call(this, 8), true);
    }
    readInt8() {
        return this.view.getInt8(__classPrivateFieldGet(this, _ReadWriteByteBuffer_instances, "m", _ReadWriteByteBuffer_roAdd).call(this, 1));
    }
    readInt16() {
        return this.view.getInt16(__classPrivateFieldGet(this, _ReadWriteByteBuffer_instances, "m", _ReadWriteByteBuffer_roAdd).call(this, 2), true);
    }
    readInt32() {
        return this.view.getInt32(__classPrivateFieldGet(this, _ReadWriteByteBuffer_instances, "m", _ReadWriteByteBuffer_roAdd).call(this, 4), true);
    }
    readInt64() {
        return this.view.getBigInt64(__classPrivateFieldGet(this, _ReadWriteByteBuffer_instances, "m", _ReadWriteByteBuffer_roAdd).call(this, 8), true);
    }
    readUint8() {
        return this.view.getUint8(__classPrivateFieldGet(this, _ReadWriteByteBuffer_instances, "m", _ReadWriteByteBuffer_roAdd).call(this, 1));
    }
    readUint16() {
        return this.view.getUint16(__classPrivateFieldGet(this, _ReadWriteByteBuffer_instances, "m", _ReadWriteByteBuffer_roAdd).call(this, 2), true);
    }
    readUint32() {
        return this.view.getUint32(__classPrivateFieldGet(this, _ReadWriteByteBuffer_instances, "m", _ReadWriteByteBuffer_roAdd).call(this, 4), true);
    }
    readUint64() {
        return this.view.getBigUint64(__classPrivateFieldGet(this, _ReadWriteByteBuffer_instances, "m", _ReadWriteByteBuffer_roAdd).call(this, 8), true);
    }
    readBuffer() {
        const length = this.view.getUint32(__classPrivateFieldGet(this, _ReadWriteByteBuffer_instances, "m", _ReadWriteByteBuffer_roAdd).call(this, 4), true);
        return this._buffer.subarray(__classPrivateFieldGet(this, _ReadWriteByteBuffer_instances, "m", _ReadWriteByteBuffer_roAdd).call(this, length), __classPrivateFieldGet(this, _ReadWriteByteBuffer_instances, "m", _ReadWriteByteBuffer_roAdd).call(this, 0));
    }
    readUtf8String() {
        const length = this.view.getUint32(__classPrivateFieldGet(this, _ReadWriteByteBuffer_instances, "m", _ReadWriteByteBuffer_roAdd).call(this, 4), true);
        return utf8Exports.read(this._buffer, __classPrivateFieldGet(this, _ReadWriteByteBuffer_instances, "m", _ReadWriteByteBuffer_roAdd).call(this, length), __classPrivateFieldGet(this, _ReadWriteByteBuffer_instances, "m", _ReadWriteByteBuffer_roAdd).call(this, 0));
    }
    incrementWriteOffset(amount) {
        return __classPrivateFieldGet(this, _ReadWriteByteBuffer_instances, "m", _ReadWriteByteBuffer_woAdd).call(this, amount);
    }
    toBinary() {
        return this._buffer.subarray(0, this.woffset);
    }
    toCopiedBinary() {
        return new Uint8Array(this.toBinary());
    }
    writeBuffer(value, writeLength = true) {
        if (writeLength) {
            this.writeUint32(value.byteLength);
        }
        const o = __classPrivateFieldGet(this, _ReadWriteByteBuffer_instances, "m", _ReadWriteByteBuffer_woAdd).call(this, value.byteLength);
        this._buffer.set(value, o);
    }
    writeUtf8String(value, writeLength = true) {
        const byteLength = utf8Exports.length(value);
        if (writeLength) {
            this.writeUint32(byteLength);
        }
        const o = __classPrivateFieldGet(this, _ReadWriteByteBuffer_instances, "m", _ReadWriteByteBuffer_woAdd).call(this, byteLength);
        utf8Exports.write(value, this._buffer, o);
    }
    writeFloat32(value) {
        const o = __classPrivateFieldGet(this, _ReadWriteByteBuffer_instances, "m", _ReadWriteByteBuffer_woAdd).call(this, 4);
        this.view.setFloat32(o, value, true);
    }
    writeFloat64(value) {
        const o = __classPrivateFieldGet(this, _ReadWriteByteBuffer_instances, "m", _ReadWriteByteBuffer_woAdd).call(this, 8);
        this.view.setFloat64(o, value, true);
    }
    writeInt8(value) {
        const o = __classPrivateFieldGet(this, _ReadWriteByteBuffer_instances, "m", _ReadWriteByteBuffer_woAdd).call(this, 1);
        this.view.setInt8(o, value, true);
    }
    writeInt16(value) {
        const o = __classPrivateFieldGet(this, _ReadWriteByteBuffer_instances, "m", _ReadWriteByteBuffer_woAdd).call(this, 2);
        this.view.setInt16(o, value, true);
    }
    writeInt32(value) {
        const o = __classPrivateFieldGet(this, _ReadWriteByteBuffer_instances, "m", _ReadWriteByteBuffer_woAdd).call(this, 4);
        this.view.setInt32(o, value, true);
    }
    writeInt64(value) {
        const o = __classPrivateFieldGet(this, _ReadWriteByteBuffer_instances, "m", _ReadWriteByteBuffer_woAdd).call(this, 8);
        this.view.setBigInt64(o, value);
    }
    writeUint8(value) {
        const o = __classPrivateFieldGet(this, _ReadWriteByteBuffer_instances, "m", _ReadWriteByteBuffer_woAdd).call(this, 1);
        this.view.setUint8(o, value);
    }
    writeUint16(value) {
        const o = __classPrivateFieldGet(this, _ReadWriteByteBuffer_instances, "m", _ReadWriteByteBuffer_woAdd).call(this, 2);
        this.view.setUint16(o, value, true);
    }
    writeUint32(value) {
        const o = __classPrivateFieldGet(this, _ReadWriteByteBuffer_instances, "m", _ReadWriteByteBuffer_woAdd).call(this, 4);
        this.view.setUint32(o, value, true);
    }
    writeUint64(value) {
        const o = __classPrivateFieldGet(this, _ReadWriteByteBuffer_instances, "m", _ReadWriteByteBuffer_woAdd).call(this, 8);
        this.view.setBigUint64(o, value, true);
    }
    // DataView Proxy
    getFloat32(offset) {
        return this.view.getFloat32(offset, true);
    }
    getFloat64(offset) {
        return this.view.getFloat64(offset, true);
    }
    getInt8(offset) {
        return this.view.getInt8(offset);
    }
    getInt16(offset) {
        return this.view.getInt16(offset, true);
    }
    getInt32(offset) {
        return this.view.getInt32(offset, true);
    }
    getInt64(offset) {
        return this.view.getBigInt64(offset, true);
    }
    getUint8(offset) {
        return this.view.getUint8(offset);
    }
    getUint16(offset) {
        return this.view.getUint16(offset, true);
    }
    getUint32(offset) {
        return this.view.getUint32(offset, true);
    }
    getUint64(offset) {
        return this.view.getBigUint64(offset, true);
    }
    setFloat32(offset, value) {
        this.view.setFloat32(offset, value, true);
    }
    setFloat64(offset, value) {
        this.view.setFloat64(offset, value, true);
    }
    setInt8(offset, value) {
        this.view.setInt8(offset, value);
    }
    setInt16(offset, value) {
        this.view.setInt16(offset, value, true);
    }
    setInt32(offset, value) {
        this.view.setInt32(offset, value, true);
    }
    setInt64(offset, value) {
        this.view.setBigInt64(offset, value, true);
    }
    setUint8(offset, value) {
        this.view.setUint8(offset, value);
    }
    setUint16(offset, value) {
        this.view.setUint16(offset, value, true);
    }
    setUint32(offset, value) {
        this.view.setUint32(offset, value, true);
    }
    setUint64(offset, value) {
        this.view.setBigUint64(offset, value, true);
    }
}
_ReadWriteByteBuffer_instances = new WeakSet(), _ReadWriteByteBuffer_woAdd = function _ReadWriteByteBuffer_woAdd(amount) {
    if (this.woffset + amount > this._buffer.byteLength) {
        const newsize = getNextSize(this._buffer.byteLength, this.woffset + amount);
        const newBuffer = new Uint8Array(newsize);
        newBuffer.set(this._buffer);
        const oldOffset = this._buffer.byteOffset;
        this._buffer = newBuffer;
        this.view = new DataView(this._buffer.buffer, oldOffset);
    }
    this.woffset += amount;
    return this.woffset - amount;
}, _ReadWriteByteBuffer_roAdd = function _ReadWriteByteBuffer_roAdd(amount) {
    if (this.roffset + amount > this.woffset) {
        throw new Error('Outside of the bounds of writen data.');
    }
    this.roffset += amount;
    return this.roffset - amount;
};

/**
 * @public
 */
var CrdtMessageType;
(function (CrdtMessageType) {
    CrdtMessageType[CrdtMessageType["RESERVED"] = 0] = "RESERVED";
    // Component Operation
    CrdtMessageType[CrdtMessageType["PUT_COMPONENT"] = 1] = "PUT_COMPONENT";
    CrdtMessageType[CrdtMessageType["DELETE_COMPONENT"] = 2] = "DELETE_COMPONENT";
    CrdtMessageType[CrdtMessageType["DELETE_ENTITY"] = 3] = "DELETE_ENTITY";
    CrdtMessageType[CrdtMessageType["APPEND_VALUE"] = 4] = "APPEND_VALUE";
    CrdtMessageType[CrdtMessageType["MAX_MESSAGE_TYPE"] = 5] = "MAX_MESSAGE_TYPE";
})(CrdtMessageType || (CrdtMessageType = {}));
/**
 * @public
 */
const CRDT_MESSAGE_HEADER_LENGTH = 8;
var ProcessMessageResultType;
(function (ProcessMessageResultType) {
    /**
     * Typical message and new state set.
     * @state CHANGE
     * @reason Incoming message has a timestamp greater
     */
    ProcessMessageResultType[ProcessMessageResultType["StateUpdatedTimestamp"] = 1] = "StateUpdatedTimestamp";
    /**
     * Typical message when it is considered old.
     * @state it does NOT CHANGE.
     * @reason incoming message has a timestamp lower.
     */
    ProcessMessageResultType[ProcessMessageResultType["StateOutdatedTimestamp"] = 2] = "StateOutdatedTimestamp";
    /**
     * Weird message, same timestamp and data.
     * @state it does NOT CHANGE.
     * @reason consistent state between peers.
     */
    ProcessMessageResultType[ProcessMessageResultType["NoChanges"] = 3] = "NoChanges";
    /**
     * Less but typical message, same timestamp, resolution by data.
     * @state it does NOT CHANGE.
     * @reason incoming message has a LOWER data.
     */
    ProcessMessageResultType[ProcessMessageResultType["StateOutdatedData"] = 4] = "StateOutdatedData";
    /**
     * Less but typical message, same timestamp, resolution by data.
     * @state CHANGE.
     * @reason incoming message has a GREATER data.
     */
    ProcessMessageResultType[ProcessMessageResultType["StateUpdatedData"] = 5] = "StateUpdatedData";
    /**
     * Entity was previously deleted.
     * @state it does NOT CHANGE.
     * @reason The message is considered old.
     */
    ProcessMessageResultType[ProcessMessageResultType["EntityWasDeleted"] = 6] = "EntityWasDeleted";
    /**
     * Entity should be deleted.
     * @state CHANGE.
     * @reason the state is storing old entities
     */
    ProcessMessageResultType[ProcessMessageResultType["EntityDeleted"] = 7] = "EntityDeleted";
})(ProcessMessageResultType || (ProcessMessageResultType = {}));
// we receive LWW, v=6, we have v=5 => we receive with delay the deleteEntity(v=5)
//   => we should generate the deleteEntity message effects internally with deleteEntity(v=5),
//       but don't resend the deleteEntity
//          - (CRDT) addDeletedEntitySet v=5 (with crdt state cleaning) and then LWW v=6
//          - (engine) engine.deleteEntity v=5
// we receive LWW, v=7, we have v=5 => we receive with delay the deleteEntity(v=5), deleteEntity(v=6), ..., N
//   => we should generate the deleteEntity message effects internally with deleteEntity(v=5),
//       but don't resend the deleteEntity
//          - (CRDT) addDeletedEntitySet v=5 (with crdt state cleaning) and then LWW v=6
//          - (engine) engine.deleteEntity v=5
// msg delete entity: it only should be sent by deleter
//

/**
 * @internal
 */
var CrdtMessageProtocol;
(function (CrdtMessageProtocol) {
    /**
     * Validate if the message incoming is completed
     * @param buf - ByteBuffer
     */
    function validate(buf) {
        const rem = buf.remainingBytes();
        if (rem < CRDT_MESSAGE_HEADER_LENGTH) {
            return false;
        }
        const messageLength = buf.getUint32(buf.currentReadOffset());
        if (rem < messageLength) {
            return false;
        }
        return true;
    }
    CrdtMessageProtocol.validate = validate;
    /**
     * Get the current header, consuming the bytes involved.
     * @param buf - ByteBuffer
     * @returns header or null if there is no validated message
     */
    function readHeader(buf) {
        if (!validate(buf)) {
            return null;
        }
        return {
            length: buf.readUint32(),
            type: buf.readUint32()
        };
    }
    CrdtMessageProtocol.readHeader = readHeader;
    /**
     * Get the current header, without consuming the bytes involved.
     * @param buf - ByteBuffer
     * @returns header or null if there is no validated message
     */
    function getHeader(buf) {
        if (!validate(buf)) {
            return null;
        }
        const currentOffset = buf.currentReadOffset();
        return {
            length: buf.getUint32(currentOffset),
            type: buf.getUint32(currentOffset + 4)
        };
    }
    CrdtMessageProtocol.getHeader = getHeader;
    /**
     * Consume the incoming message without processing it.
     * @param buf - ByteBuffer
     * @returns true in case of success or false if there is no valid message.
     */
    function consumeMessage(buf) {
        const header = getHeader(buf);
        if (!header) {
            return false;
        }
        buf.incrementReadOffset(header.length);
        return true;
    }
    CrdtMessageProtocol.consumeMessage = consumeMessage;
})(CrdtMessageProtocol || (CrdtMessageProtocol = {}));

/**
 * @internal
 */
var DeleteComponent;
(function (DeleteComponent) {
    DeleteComponent.MESSAGE_HEADER_LENGTH = 12;
    /**
     * Write DeleteComponent message
     */
    function write(entity, componentId, timestamp, buf) {
        // reserve the beginning
        const messageLength = CRDT_MESSAGE_HEADER_LENGTH + DeleteComponent.MESSAGE_HEADER_LENGTH;
        const startMessageOffset = buf.incrementWriteOffset(messageLength);
        // Write CrdtMessage header
        buf.setUint32(startMessageOffset, messageLength);
        buf.setUint32(startMessageOffset + 4, CrdtMessageType.DELETE_COMPONENT);
        // Write ComponentOperation header
        buf.setUint32(startMessageOffset + 8, entity);
        buf.setUint32(startMessageOffset + 12, componentId);
        buf.setUint32(startMessageOffset + 16, timestamp);
    }
    DeleteComponent.write = write;
    function read(buf) {
        const header = CrdtMessageProtocol.readHeader(buf);
        if (!header) {
            return null;
        }
        if (header.type !== CrdtMessageType.DELETE_COMPONENT) {
            throw new Error('DeleteComponentOperation tried to read another message type.');
        }
        const msg = {
            ...header,
            entityId: buf.readUint32(),
            componentId: buf.readUint32(),
            timestamp: buf.readUint32()
        };
        return msg;
    }
    DeleteComponent.read = read;
})(DeleteComponent || (DeleteComponent = {}));

/**
 * @internal
 */
var AppendValueOperation;
(function (AppendValueOperation) {
    AppendValueOperation.MESSAGE_HEADER_LENGTH = 16;
    /**
     * Call this function for an optimal writing data passing the ByteBuffer
     *  already allocated
     */
    function write(entity, timestamp, componentId, data, buf) {
        // reserve the beginning
        const startMessageOffset = buf.incrementWriteOffset(CRDT_MESSAGE_HEADER_LENGTH + AppendValueOperation.MESSAGE_HEADER_LENGTH);
        // write body
        buf.writeBuffer(data, false);
        const messageLength = buf.currentWriteOffset() - startMessageOffset;
        // Write CrdtMessage header
        buf.setUint32(startMessageOffset, messageLength);
        buf.setUint32(startMessageOffset + 4, CrdtMessageType.APPEND_VALUE);
        // Write ComponentOperation header
        buf.setUint32(startMessageOffset + 8, entity);
        buf.setUint32(startMessageOffset + 12, componentId);
        buf.setUint32(startMessageOffset + 16, timestamp);
        const newLocal = messageLength - AppendValueOperation.MESSAGE_HEADER_LENGTH - CRDT_MESSAGE_HEADER_LENGTH;
        buf.setUint32(startMessageOffset + 20, newLocal);
    }
    AppendValueOperation.write = write;
    function read(buf) {
        const header = CrdtMessageProtocol.readHeader(buf);
        /* istanbul ignore if */
        if (!header) {
            return null;
        }
        /* istanbul ignore if */
        if (header.type !== CrdtMessageType.APPEND_VALUE) {
            throw new Error('AppendValueOperation tried to read another message type.');
        }
        return {
            ...header,
            entityId: buf.readUint32(),
            componentId: buf.readUint32(),
            timestamp: buf.readUint32(),
            data: buf.readBuffer()
        };
    }
    AppendValueOperation.read = read;
})(AppendValueOperation || (AppendValueOperation = {}));

/**
 * @internal
 */
var DeleteEntity;
(function (DeleteEntity) {
    DeleteEntity.MESSAGE_HEADER_LENGTH = 4;
    function write(entity, buf) {
        // Write CrdtMessage header
        buf.writeUint32(CRDT_MESSAGE_HEADER_LENGTH + 4);
        buf.writeUint32(CrdtMessageType.DELETE_ENTITY);
        // body
        buf.writeUint32(entity);
    }
    DeleteEntity.write = write;
    function read(buf) {
        const header = CrdtMessageProtocol.readHeader(buf);
        if (!header) {
            return null;
        }
        if (header.type !== CrdtMessageType.DELETE_ENTITY) {
            throw new Error('DeleteEntity tried to read another message type.');
        }
        return {
            ...header,
            entityId: buf.readUint32()
        };
    }
    DeleteEntity.read = read;
})(DeleteEntity || (DeleteEntity = {}));

/**
 * @internal
 */
var PutComponentOperation;
(function (PutComponentOperation) {
    PutComponentOperation.MESSAGE_HEADER_LENGTH = 16;
    /**
     * Call this function for an optimal writing data passing the ByteBuffer
     *  already allocated
     */
    function write(entity, timestamp, componentId, data, buf) {
        // reserve the beginning
        const startMessageOffset = buf.incrementWriteOffset(CRDT_MESSAGE_HEADER_LENGTH + PutComponentOperation.MESSAGE_HEADER_LENGTH);
        // write body
        buf.writeBuffer(data, false);
        const messageLength = buf.currentWriteOffset() - startMessageOffset;
        // Write CrdtMessage header
        buf.setUint32(startMessageOffset, messageLength);
        buf.setUint32(startMessageOffset + 4, CrdtMessageType.PUT_COMPONENT);
        // Write ComponentOperation header
        buf.setUint32(startMessageOffset + 8, entity);
        buf.setUint32(startMessageOffset + 12, componentId);
        buf.setUint32(startMessageOffset + 16, timestamp);
        const newLocal = messageLength - PutComponentOperation.MESSAGE_HEADER_LENGTH - CRDT_MESSAGE_HEADER_LENGTH;
        buf.setUint32(startMessageOffset + 20, newLocal);
    }
    PutComponentOperation.write = write;
    function read(buf) {
        const header = CrdtMessageProtocol.readHeader(buf);
        if (!header) {
            return null;
        }
        if (header.type !== CrdtMessageType.PUT_COMPONENT) {
            throw new Error('PutComponentOperation tried to read another message type.');
        }
        return {
            ...header,
            entityId: buf.readUint32(),
            componentId: buf.readUint32(),
            timestamp: buf.readUint32(),
            data: buf.readBuffer()
        };
    }
    PutComponentOperation.read = read;
})(PutComponentOperation || (PutComponentOperation = {}));

/**
 * @internal
 */
function crdtSceneSystem(engine, onProcessEntityComponentChange) {
    const transports = [];
    // Messages that we received at transport.onMessage waiting to be processed
    const receivedMessages = [];
    // Messages already processed by the engine but that we need to broadcast to other transports.
    const broadcastMessages = [];
    // Messages receieved by a transport that were outdated. We need to correct them
    const outdatedMessages = [];
    /**
     *
     * @param transportId tranport id to identiy messages
     * @returns a function to process received messages
     */
    function parseChunkMessage(transportId) {
        /**
         * Receives a chunk of binary messages and stores all the valid
         * Component Operation Messages at messages queue
         * @param chunkMessage A chunk of binary messages
         */
        return function parseChunkMessage(chunkMessage) {
            const buffer = new ReadWriteByteBuffer(chunkMessage);
            let header;
            while ((header = CrdtMessageProtocol.getHeader(buffer))) {
                const offset = buffer.currentReadOffset();
                if (header.type === CrdtMessageType.DELETE_COMPONENT) {
                    const message = DeleteComponent.read(buffer);
                    receivedMessages.push({
                        ...message,
                        transportId,
                        messageBuffer: buffer.buffer().subarray(offset, buffer.currentReadOffset())
                    });
                }
                else if (header.type === CrdtMessageType.PUT_COMPONENT) {
                    const message = PutComponentOperation.read(buffer);
                    receivedMessages.push({
                        ...message,
                        transportId,
                        messageBuffer: buffer.buffer().subarray(offset, buffer.currentReadOffset())
                    });
                }
                else if (header.type === CrdtMessageType.DELETE_ENTITY) {
                    const message = DeleteEntity.read(buffer);
                    receivedMessages.push({
                        ...message,
                        transportId,
                        messageBuffer: buffer.buffer().subarray(offset, buffer.currentReadOffset())
                    });
                }
                else if (header.type === CrdtMessageType.APPEND_VALUE) {
                    const message = AppendValueOperation.read(buffer);
                    receivedMessages.push({
                        ...message,
                        transportId,
                        messageBuffer: buffer.buffer().subarray(offset, buffer.currentReadOffset())
                    });
                    // Unknown message, we skip it
                }
                else {
                    // consume the message
                    buffer.incrementReadOffset(header.length);
                }
            }
            // TODO: do something if buffler.len>0
        };
    }
    /**
     * Return and clear the messaes queue
     * @returns messages recieved by the transport to process on the next tick
     */
    function getMessages(value) {
        const messagesToProcess = value.splice(0, value.length);
        return messagesToProcess;
    }
    /**
     * This fn will be called on every tick.
     * Process all the messages queue received by the transport
     */
    async function receiveMessages() {
        const messagesToProcess = getMessages(receivedMessages);
        const bufferForOutdated = new ReadWriteByteBuffer();
        const entitiesShouldBeCleaned = [];
        for (const msg of messagesToProcess) {
            if (msg.type === CrdtMessageType.DELETE_ENTITY) {
                entitiesShouldBeCleaned.push(msg.entityId);
            }
            else {
                const entityState = engine.entityContainer.getEntityState(msg.entityId);
                // Skip updates from removed entityes
                if (entityState === EntityState.Removed)
                    continue;
                // Entities with unknown entities should update its entity state
                if (entityState === EntityState.Unknown) {
                    engine.entityContainer.updateUsedEntity(msg.entityId);
                }
                const component = engine.getComponentOrNull(msg.componentId);
                if (component) {
                    const [conflictMessage, value] = component.updateFromCrdt(msg);
                    if (conflictMessage) {
                        const offset = bufferForOutdated.currentWriteOffset();
                        if (conflictMessage.type === CrdtMessageType.PUT_COMPONENT) {
                            PutComponentOperation.write(msg.entityId, conflictMessage.timestamp, conflictMessage.componentId, conflictMessage.data, bufferForOutdated);
                        }
                        else if (conflictMessage.type === CrdtMessageType.DELETE_COMPONENT) {
                            DeleteComponent.write(msg.entityId, component.componentId, conflictMessage.timestamp, bufferForOutdated);
                        }
                        outdatedMessages.push({
                            ...msg,
                            messageBuffer: bufferForOutdated.buffer().subarray(offset, bufferForOutdated.currentWriteOffset())
                        });
                    }
                    else {
                        // Add message to transport queue to be processed by others transports
                        broadcastMessages.push(msg);
                        onProcessEntityComponentChange && onProcessEntityComponentChange(msg.entityId, msg.type, component, value);
                    }
                }
            }
        }
        // the last stage of the syncrhonization is to delete the entities
        for (const entity of entitiesShouldBeCleaned) {
            // If we tried to resend outdated message and the entity was deleted before, we avoid sending them.
            for (let i = outdatedMessages.length - 1; i >= 0; i--) {
                if (outdatedMessages[i].entityId === entity && outdatedMessages[i].type !== CrdtMessageType.DELETE_ENTITY) {
                    outdatedMessages.splice(i, 1);
                }
            }
            for (const definition of engine.componentsIter()) {
                definition.entityDeleted(entity, false);
            }
            engine.entityContainer.updateRemovedEntity(entity);
            onProcessEntityComponentChange && onProcessEntityComponentChange(entity, CrdtMessageType.DELETE_ENTITY);
        }
    }
    /**
     * Iterates the dirty map and generates crdt messages to be send
     */
    async function sendMessages(entitiesDeletedThisTick) {
        // CRDT Messages will be the merge between the recieved transport messages and the new crdt messages
        const crdtMessages = getMessages(broadcastMessages);
        const outdatedMessagesBkp = getMessages(outdatedMessages);
        const buffer = new ReadWriteByteBuffer();
        for (const component of engine.componentsIter()) {
            for (const message of component.getCrdtUpdates()) {
                const offset = buffer.currentWriteOffset();
                // Avoid creating messages if there is no transport that will handle it
                if (transports.some((t) => t.filter(message))) {
                    if (message.type === CrdtMessageType.PUT_COMPONENT) {
                        PutComponentOperation.write(message.entityId, message.timestamp, message.componentId, message.data, buffer);
                    }
                    else if (message.type === CrdtMessageType.DELETE_COMPONENT) {
                        DeleteComponent.write(message.entityId, component.componentId, message.timestamp, buffer);
                    }
                    else if (message.type === CrdtMessageType.APPEND_VALUE) {
                        AppendValueOperation.write(message.entityId, message.timestamp, message.componentId, message.data, buffer);
                    }
                    crdtMessages.push({
                        ...message,
                        messageBuffer: buffer.buffer().subarray(offset, buffer.currentWriteOffset())
                    });
                    if (onProcessEntityComponentChange) {
                        const rawValue = message.type === CrdtMessageType.PUT_COMPONENT || message.type === CrdtMessageType.APPEND_VALUE
                            ? component.get(message.entityId)
                            : undefined;
                        onProcessEntityComponentChange(message.entityId, message.type, component, rawValue);
                    }
                }
            }
        }
        // After all updates, I execute the DeletedEntity messages
        for (const entityId of entitiesDeletedThisTick) {
            const offset = buffer.currentWriteOffset();
            DeleteEntity.write(entityId, buffer);
            crdtMessages.push({
                type: CrdtMessageType.DELETE_ENTITY,
                entityId,
                messageBuffer: buffer.buffer().subarray(offset, buffer.currentWriteOffset())
            });
            onProcessEntityComponentChange && onProcessEntityComponentChange(entityId, CrdtMessageType.DELETE_ENTITY);
        }
        // Send CRDT messages to transports
        const transportBuffer = new ReadWriteByteBuffer();
        for (const index in transports) {
            const transportIndex = Number(index);
            const transport = transports[transportIndex];
            transportBuffer.resetBuffer();
            // First we need to send all the messages that were outdated from a transport
            // So we can fix their crdt state
            for (const message of outdatedMessagesBkp) {
                if (message.transportId === transportIndex &&
                    // TODO: This is an optimization, the state should converge anyway, whatever the message is sent.
                    // Avoid sending multiple messages for the same entity-componentId
                    !crdtMessages.find((m) => m.entityId === message.entityId &&
                        // TODO: as any, with multiple type of messages, it should have many checks before the check for similar messages
                        m.componentId &&
                        m.componentId === message.componentId)) {
                    transportBuffer.writeBuffer(message.messageBuffer, false);
                }
            }
            // Then we send all the new crdtMessages that the transport needs to process
            for (const message of crdtMessages) {
                if (message.transportId !== transportIndex && transport.filter(message)) {
                    transportBuffer.writeBuffer(message.messageBuffer, false);
                }
            }
            const message = transportBuffer.currentWriteOffset() ? transportBuffer.toBinary() : new Uint8Array([]);
            await transport.send(message);
        }
    }
    /**
     * @public
     * Add a transport to the crdt system
     */
    function addTransport(transport) {
        const id = transports.push(transport) - 1;
        transport.onmessage = parseChunkMessage(id);
    }
    return {
        sendMessages,
        receiveMessages,
        addTransport
    };
}

var CrdtUtils;
(function (CrdtUtils) {
    (function (SynchronizedEntityType) {
        // synchronizes entities with the NetworkSynchronized component only, used for networked games
        SynchronizedEntityType[SynchronizedEntityType["NETWORKED"] = 0] = "NETWORKED";
        // synchronizes entities needed by the renderer
        SynchronizedEntityType[SynchronizedEntityType["RENDERER"] = 1] = "RENDERER";
    })(CrdtUtils.SynchronizedEntityType || (CrdtUtils.SynchronizedEntityType = {}));
})(CrdtUtils || (CrdtUtils = {}));
/**
 * Compare raw data.
 * @internal
 * @returns 0 if is the same data, 1 if a > b, -1 if b > a
 */
function dataCompare(a, b) {
    // At reference level
    if (a === b)
        return 0;
    if (a === null && b !== null)
        return -1;
    if (a !== null && b === null)
        return 1;
    if (a instanceof Uint8Array && b instanceof Uint8Array) {
        let res;
        const n = a.byteLength > b.byteLength ? b.byteLength : a.byteLength;
        for (let i = 0; i < n; i++) {
            res = a[i] - b[i];
            if (res !== 0) {
                return res > 0 ? 1 : -1;
            }
        }
        res = a.byteLength - b.byteLength;
        return res > 0 ? 1 : res < 0 ? -1 : 0;
    }
    if (typeof a === 'string') {
        return a.localeCompare(b);
    }
    return a > b ? 1 : -1;
}

/**
 * @internal
 */
function deepReadonly(val) {
    return Object.freeze({ ...val });
}

function incrementTimestamp(entity, timestamps) {
    const newTimestamp = (timestamps.get(entity) || 0) + 1;
    timestamps.set(entity, newTimestamp);
    return newTimestamp;
}
function createUpdateLwwFromCrdt(componentId, timestamps, schema, data) {
    /**
     * Process the received message only if the lamport number recieved is higher
     * than the stored one. If its lower, we spread it to the network to correct the peer.
     * If they are equal, the bigger raw data wins.
  
      * Returns the recieved data if the lamport number was bigger than ours.
      * If it was an outdated message, then we return void
      * @public
      */
    function crdtRuleForCurrentState(message) {
        const { entityId, timestamp } = message;
        const currentTimestamp = timestamps.get(entityId);
        // The received message is > than our current value, update our state.components.
        if (currentTimestamp === undefined || currentTimestamp < timestamp) {
            return ProcessMessageResultType.StateUpdatedTimestamp;
        }
        // Outdated Message. Resend our state message through the wire.
        if (currentTimestamp > timestamp) {
            // console.log('2', currentTimestamp, timestamp)
            return ProcessMessageResultType.StateOutdatedTimestamp;
        }
        // Deletes are idempotent
        if (message.type === CrdtMessageType.DELETE_COMPONENT && !data.has(entityId)) {
            return ProcessMessageResultType.NoChanges;
        }
        let currentDataGreater = 0;
        if (data.has(entityId)) {
            const writeBuffer = new ReadWriteByteBuffer();
            schema.serialize(data.get(entityId), writeBuffer);
            currentDataGreater = dataCompare(writeBuffer.toBinary(), message.data || null);
        }
        else {
            currentDataGreater = dataCompare(null, message.data);
        }
        // Same data, same timestamp. Weirdo echo message.
        // console.log('3', currentDataGreater, writeBuffer.toBinary(), (message as any).data || null)
        if (currentDataGreater === 0) {
            return ProcessMessageResultType.NoChanges;
        }
        else if (currentDataGreater > 0) {
            // Current data is greater
            return ProcessMessageResultType.StateOutdatedData;
        }
        else {
            // Curent data is lower
            return ProcessMessageResultType.StateUpdatedData;
        }
    }
    return (msg) => {
        /* istanbul ignore next */
        if (msg.type !== CrdtMessageType.PUT_COMPONENT && msg.type !== CrdtMessageType.DELETE_COMPONENT)
            /* istanbul ignore next */
            return [null, data.get(msg.entityId)];
        const action = crdtRuleForCurrentState(msg);
        const entity = msg.entityId;
        switch (action) {
            case ProcessMessageResultType.StateUpdatedData:
            case ProcessMessageResultType.StateUpdatedTimestamp: {
                timestamps.set(entity, msg.timestamp);
                if (msg.type === CrdtMessageType.PUT_COMPONENT) {
                    const buf = new ReadWriteByteBuffer(msg.data);
                    data.set(entity, schema.deserialize(buf));
                }
                else {
                    data.delete(entity);
                }
                return [null, data.get(entity)];
            }
            case ProcessMessageResultType.StateOutdatedTimestamp:
            case ProcessMessageResultType.StateOutdatedData: {
                if (data.has(entity)) {
                    const writeBuffer = new ReadWriteByteBuffer();
                    schema.serialize(data.get(entity), writeBuffer);
                    return [
                        {
                            type: CrdtMessageType.PUT_COMPONENT,
                            componentId,
                            data: writeBuffer.toBinary(),
                            entityId: entity,
                            timestamp: timestamps.get(entity)
                        },
                        data.get(entity)
                    ];
                }
                else {
                    return [
                        {
                            type: CrdtMessageType.DELETE_COMPONENT,
                            componentId,
                            entityId: entity,
                            timestamp: timestamps.get(entity)
                        },
                        undefined
                    ];
                }
            }
        }
        return [null, data.get(entity)];
    };
}
function createGetCrdtMessagesForLww(componentId, timestamps, dirtyIterator, schema, data) {
    return function* () {
        for (const entity of dirtyIterator) {
            const newTimestamp = incrementTimestamp(entity, timestamps);
            if (data.has(entity)) {
                const writeBuffer = new ReadWriteByteBuffer();
                schema.serialize(data.get(entity), writeBuffer);
                const msg = {
                    type: CrdtMessageType.PUT_COMPONENT,
                    componentId,
                    entityId: entity,
                    data: writeBuffer.toBinary(),
                    timestamp: newTimestamp
                };
                yield msg;
            }
            else {
                const msg = {
                    type: CrdtMessageType.DELETE_COMPONENT,
                    componentId,
                    entityId: entity,
                    timestamp: newTimestamp
                };
                yield msg;
            }
        }
        dirtyIterator.clear();
    };
}
/**
 * @internal
 */
function createComponentDefinitionFromSchema(componentName, componentId, schema) {
    const data = new Map();
    const dirtyIterator = new Set();
    const timestamps = new Map();
    return {
        get componentId() {
            return componentId;
        },
        get componentName() {
            return componentName;
        },
        get componentType() {
            // a getter is used here to prevent accidental changes
            return 0 /* ComponentType.LastWriteWinElementSet */;
        },
        schema,
        has(entity) {
            return data.has(entity);
        },
        deleteFrom(entity, markAsDirty = true) {
            const component = data.get(entity);
            if (data.delete(entity) && markAsDirty) {
                dirtyIterator.add(entity);
            }
            return component || null;
        },
        entityDeleted(entity, markAsDirty) {
            if (data.delete(entity) && markAsDirty) {
                dirtyIterator.add(entity);
            }
        },
        getOrNull(entity) {
            const component = data.get(entity);
            return component ? deepReadonly(component) : null;
        },
        get(entity) {
            const component = data.get(entity);
            if (!component) {
                throw new Error(`[getFrom] Component ${componentName} for entity #${entity} not found`);
            }
            return deepReadonly(component);
        },
        create(entity, value) {
            const component = data.get(entity);
            if (component) {
                throw new Error(`[create] Component ${componentName} for ${entity} already exists`);
            }
            const usedValue = value === undefined ? schema.create() : schema.extend ? schema.extend(value) : value;
            data.set(entity, usedValue);
            dirtyIterator.add(entity);
            return usedValue;
        },
        createOrReplace(entity, value) {
            const usedValue = value === undefined ? schema.create() : schema.extend ? schema.extend(value) : value;
            data.set(entity, usedValue);
            dirtyIterator.add(entity);
            return usedValue;
        },
        getMutableOrNull(entity) {
            const component = data.get(entity);
            if (!component) {
                return null;
            }
            dirtyIterator.add(entity);
            return component;
        },
        getMutable(entity) {
            const component = this.getMutableOrNull(entity);
            if (component === null) {
                throw new Error(`[mutable] Component ${componentName} for ${entity} not found`);
            }
            return component;
        },
        *iterator() {
            for (const [entity, component] of data) {
                yield [entity, component];
            }
        },
        *dirtyIterator() {
            for (const entity of dirtyIterator) {
                yield entity;
            }
        },
        getCrdtUpdates: createGetCrdtMessagesForLww(componentId, timestamps, dirtyIterator, schema, data),
        updateFromCrdt: createUpdateLwwFromCrdt(componentId, timestamps, schema, data)
    };
}

const SYSTEMS_REGULAR_PRIORITY = 100e3;
function SystemContainer() {
    const systems = [];
    function sort() {
        // TODO: systems with the same priority should always have the same stable order
        //       add a "counter" to the System type to ensure that order
        systems.sort((a, b) => b.priority - a.priority);
    }
    function add(fn, priority, name) {
        const systemName = name ?? fn.name;
        if (systems.find((item) => item.fn === fn)) {
            throw new Error(`System ${JSON.stringify(systemName)} already added to the engine`);
        }
        systems.push({
            fn,
            priority,
            name: systemName
        });
        // TODO: replace this sort by an insertion in the right place
        sort();
    }
    function remove(selector) {
        let index = -1;
        if (typeof selector === 'string') {
            index = systems.findIndex((item) => item.name === selector);
        }
        else {
            index = systems.findIndex((item) => item.fn === selector);
        }
        if (index === -1) {
            return false;
        }
        systems.splice(index, 1);
        sort();
        return true;
    }
    return {
        add,
        remove,
        getSystems() {
            return systems;
        }
    };
}

const emptyReadonlySet = freezeSet(new Set());
function frozenError() {
    throw new Error('The set is frozen');
}
function freezeSet(set) {
    set.add = frozenError;
    set.clear = frozenError;
    return set;
}
function sortByTimestamp(a, b) {
    return a.timestamp > b.timestamp ? 1 : -1;
}
/**
 * @internal
 */
function createValueSetComponentDefinitionFromSchema(componentName, componentId, schema, options) {
    const data = new Map();
    const dirtyIterator = new Set();
    const queuedCommands = [];
    // only sort the array if the latest (N) element has a timestamp <= N-1
    function shouldSort(row) {
        const len = row.raw.length;
        if (len > 1 && row.raw[len - 1].timestamp <= row.raw[len - 2].timestamp) {
            return true;
        }
        return false;
    }
    function gotUpdated(entity) {
        const row = data.get(entity);
        /* istanbul ignore else */
        if (row) {
            if (shouldSort(row)) {
                row.raw.sort(sortByTimestamp);
            }
            while (row.raw.length > options.maxElements) {
                row.raw.shift();
            }
            const frozenSet = freezeSet(new Set(row?.raw.map(($) => $.value)));
            row.frozenSet = frozenSet;
            return frozenSet;
        }
        else {
            /* istanbul ignore next */
            return emptyReadonlySet;
        }
    }
    function append(entity, value) {
        let row = data.get(entity);
        if (!row) {
            row = { raw: [], frozenSet: emptyReadonlySet };
            data.set(entity, row);
        }
        const usedValue = schema.extend ? schema.extend(value) : value;
        const timestamp = options.timestampFunction(usedValue);
        {
            // only freeze the objects in dev mode to warn the developers because
            // it is an expensive operation
            Object.freeze(usedValue);
        }
        row.raw.push({ value: usedValue, timestamp });
        return { set: gotUpdated(entity), value: usedValue };
    }
    const ret = {
        get componentId() {
            return componentId;
        },
        get componentName() {
            return componentName;
        },
        get componentType() {
            // a getter is used here to prevent accidental changes
            return 1 /* ComponentType.GrowOnlyValueSet */;
        },
        schema,
        has(entity) {
            return data.has(entity);
        },
        entityDeleted(entity) {
            data.delete(entity);
        },
        get(entity) {
            const values = data.get(entity);
            if (values) {
                return values.frozenSet;
            }
            else {
                return emptyReadonlySet;
            }
        },
        addValue(entity, rawValue) {
            const { set, value } = append(entity, rawValue);
            dirtyIterator.add(entity);
            const buf = new ReadWriteByteBuffer();
            schema.serialize(value, buf);
            queuedCommands.push({
                componentId,
                data: buf.toBinary(),
                entityId: entity,
                timestamp: 0,
                type: CrdtMessageType.APPEND_VALUE
            });
            return set;
        },
        *iterator() {
            for (const [entity, component] of data) {
                yield [entity, component.frozenSet];
            }
        },
        *dirtyIterator() {
            for (const entity of dirtyIterator) {
                yield entity;
            }
        },
        getCrdtUpdates() {
            // return a copy of the commands, and then clear the local copy
            dirtyIterator.clear();
            return queuedCommands.splice(0, queuedCommands.length);
        },
        updateFromCrdt(_body) {
            if (_body.type === CrdtMessageType.APPEND_VALUE) {
                const buf = new ReadWriteByteBuffer(_body.data);
                append(_body.entityId, schema.deserialize(buf));
            }
            return [null, undefined];
        }
    };
    return ret;
}

const InputCommands = [
    0 /* InputAction.IA_POINTER */,
    1 /* InputAction.IA_PRIMARY */,
    2 /* InputAction.IA_SECONDARY */,
    4 /* InputAction.IA_FORWARD */,
    5 /* InputAction.IA_BACKWARD */,
    6 /* InputAction.IA_RIGHT */,
    7 /* InputAction.IA_LEFT */,
    8 /* InputAction.IA_JUMP */,
    9 /* InputAction.IA_WALK */,
    10 /* InputAction.IA_ACTION_3 */,
    11 /* InputAction.IA_ACTION_4 */,
    12 /* InputAction.IA_ACTION_5 */,
    13 /* InputAction.IA_ACTION_6 */
];
const InputStateUpdateSystemPriority = 1 << 20;
/**
 * @internal
 */
function createInputSystem(engine) {
    const PointerEventsResult$1 = PointerEventsResult(engine);
    const globalState = {
        previousFrameMaxTimestamp: 0,
        currentFrameMaxTimestamp: 0,
        buttonState: new Map(),
        thisFrameCommands: []
    };
    function findLastAction(pointerEventType, inputAction, entity) {
        const ascendingTimestampIterator = PointerEventsResult$1.get(entity);
        for (const command of Array.from(ascendingTimestampIterator).reverse()) {
            if (command.button === inputAction && command.state === pointerEventType) {
                return command;
            }
        }
    }
    function* findCommandsByActionDescending(inputAction, entity) {
        const ascendingTimestampIterator = PointerEventsResult$1.get(entity);
        for (const command of Array.from(ascendingTimestampIterator).reverse()) {
            if (command.button === inputAction) {
                yield command;
            }
        }
    }
    function buttonStateUpdateSystem() {
        // first store the previous' frame timestamp
        let maxTimestamp = globalState.currentFrameMaxTimestamp;
        globalState.previousFrameMaxTimestamp = maxTimestamp;
        if (globalState.thisFrameCommands.length) {
            globalState.thisFrameCommands = [];
        }
        // then iterate over all new commands
        for (const [, commands] of engine.getEntitiesWith(PointerEventsResult$1)) {
            // TODO: adapt the gset component to have a cached "reversed" option by default
            const arrayCommands = Array.from(commands);
            for (let i = arrayCommands.length - 1; i >= 0; i--) {
                const command = arrayCommands[i];
                if (command.timestamp > maxTimestamp) {
                    maxTimestamp = command.timestamp;
                }
                if (command.timestamp > globalState.previousFrameMaxTimestamp) {
                    globalState.thisFrameCommands.push(command);
                }
                if (command.state === 0 /* PointerEventType.PET_UP */ || command.state === 1 /* PointerEventType.PET_DOWN */) {
                    const prevCommand = globalState.buttonState.get(command.button);
                    if (!prevCommand || command.timestamp > prevCommand.timestamp) {
                        globalState.buttonState.set(command.button, command);
                    }
                    else {
                        // since we are iterating a descending array, we can early finish the
                        // loop
                        break;
                    }
                }
            }
        }
        // update current frame's max timestamp
        globalState.currentFrameMaxTimestamp = maxTimestamp;
    }
    engine.addSystem(buttonStateUpdateSystem, InputStateUpdateSystemPriority, '@dcl/ecs#inputSystem');
    function timestampIsCurrentFrame(timestamp) {
        if (timestamp > globalState.previousFrameMaxTimestamp && timestamp <= globalState.currentFrameMaxTimestamp) {
            return true;
        }
        else {
            return false;
        }
    }
    function getClick(inputAction, entity) {
        if (inputAction !== 3 /* InputAction.IA_ANY */) {
            return findClick(inputAction, entity);
        }
        for (const input of InputCommands) {
            const cmd = findClick(input, entity);
            if (cmd)
                return cmd;
        }
        return null;
    }
    function findClick(inputAction, entity) {
        let down = null;
        let up = null;
        // We search the last UP & DOWN command sorted by timestamp descending
        for (const it of findCommandsByActionDescending(inputAction, entity)) {
            if (!up) {
                if (it.state === 0 /* PointerEventType.PET_UP */) {
                    up = it;
                    continue;
                }
            }
            else if (!down) {
                if (it.state === 1 /* PointerEventType.PET_DOWN */) {
                    down = it;
                    break;
                }
            }
        }
        if (!up || !down)
            return null;
        // If the DOWN command has happen before the UP commands, it means that that a clicked has happen
        if (down.timestamp < up.timestamp && timestampIsCurrentFrame(up.timestamp)) {
            return { up, down };
        }
        return null;
    }
    function getInputCommandFromEntity(inputAction, pointerEventType, entity) {
        if (inputAction !== 3 /* InputAction.IA_ANY */) {
            return findInputCommand(inputAction, pointerEventType, entity);
        }
        for (const input of InputCommands) {
            const cmd = findInputCommand(input, pointerEventType, entity);
            if (cmd)
                return cmd;
        }
        return null;
    }
    function getInputCommand(inputAction, pointerEventType, entity) {
        if (entity) {
            return getInputCommandFromEntity(inputAction, pointerEventType, entity);
        }
        else {
            for (const command of globalState.thisFrameCommands) {
                if (command.button === inputAction && command.state === pointerEventType) {
                    return command;
                }
            }
            return null;
        }
    }
    function findInputCommand(inputAction, pointerEventType, entity) {
        // We search the last pointer Event command sorted by timestamp
        const command = findLastAction(pointerEventType, inputAction, entity);
        if (!command)
            return null;
        if (timestampIsCurrentFrame(command.timestamp)) {
            return command;
        }
        else {
            return null;
        }
    }
    // returns true if there was a DOWN (in any past frame), and then an UP in the last frame
    function isClicked(inputAction, entity) {
        return getClick(inputAction, entity) !== null;
    }
    // returns true if the provided last action was triggered in the last frame
    function isTriggered(inputAction, pointerEventType, entity) {
        if (entity) {
            const command = findLastAction(pointerEventType, inputAction, entity);
            return (command && timestampIsCurrentFrame(command.timestamp)) || false;
        }
        else {
            for (const command of globalState.thisFrameCommands) {
                if (command.button === inputAction && command.state === pointerEventType) {
                    return true;
                }
            }
            return false;
        }
    }
    // returns the global state of the input. This global state is updated from the system
    function isPressed(inputAction) {
        return globalState.buttonState.get(inputAction)?.state === 1 /* PointerEventType.PET_DOWN */;
    }
    return {
        isPressed,
        getClick,
        getInputCommand,
        isClicked,
        isTriggered
    };
}

function preEngine() {
    const entityContainer = EntityContainer();
    const componentsDefinition = new Map();
    const systems = SystemContainer();
    let sealed = false;
    function addSystem(fn, priority = SYSTEMS_REGULAR_PRIORITY, name) {
        systems.add(fn, priority, name);
    }
    function removeSystem(selector) {
        return systems.remove(selector);
    }
    function addEntity() {
        const entity = entityContainer.generateEntity();
        return entity;
    }
    function removeEntity(entity) {
        for (const [, component] of componentsDefinition) {
            component.entityDeleted(entity, true);
        }
        return entityContainer.removeEntity(entity);
    }
    function registerComponentDefinition(componentName, component) {
        /* istanbul ignore next */
        if (sealed)
            throw new Error('Engine is already sealed. No components can be added at this stage');
        const componentId = componentNumberFromName(componentName);
        const prev = componentsDefinition.get(componentId);
        if (prev) {
            throw new Error(`Component number ${componentId} was already registered.`);
        }
        /* istanbul ignore next */
        if (component.componentName !== componentName) {
            throw new Error(`Component name doesn't match componentDefinition.componentName ${componentName} != ${component.componentName}`);
        }
        /* istanbul ignore next */
        if (component.componentId !== componentId) {
            throw new Error(`Component number doesn't match componentDefinition.componentId ${componentId} != ${component.componentId}`);
        }
        componentsDefinition.set(componentId, component);
        return component;
    }
    function defineComponentFromSchema(componentName, schema) {
        /* istanbul ignore next */
        if (sealed)
            throw new Error('Engine is already sealed. No components can be added at this stage');
        const componentId = componentNumberFromName(componentName);
        const prev = componentsDefinition.get(componentId);
        if (prev) {
            // TODO: assert spec === prev.spec
            return prev;
        }
        const newComponent = createComponentDefinitionFromSchema(componentName, componentId, schema);
        componentsDefinition.set(componentId, newComponent);
        return newComponent;
    }
    function defineValueSetComponentFromSchema(componentName, schema, options) {
        /* istanbul ignore next */
        if (sealed)
            throw new Error('Engine is already sealed. No components can be added at this stage');
        const componentId = componentNumberFromName(componentName);
        const prev = componentsDefinition.get(componentId);
        if (prev) {
            // TODO: assert spec === prev.spec
            return prev;
        }
        const newComponent = createValueSetComponentDefinitionFromSchema(componentName, componentId, schema, options);
        componentsDefinition.set(componentId, newComponent);
        return newComponent;
    }
    function defineComponent(componentName, mapSpec, constructorDefault) {
        if (sealed)
            throw new Error('Engine is already sealed. No components can be added at this stage');
        const componentId = componentNumberFromName(componentName);
        const prev = componentsDefinition.get(componentId);
        if (prev) {
            // TODO: assert spec === prev.spec
            return prev;
        }
        const schemaSpec = Schemas.Map(mapSpec, constructorDefault);
        const def = createComponentDefinitionFromSchema(componentName, componentId, schemaSpec);
        const newComponent = {
            ...def,
            create(entity, val) {
                return def.create(entity, val);
            },
            createOrReplace(entity, val) {
                return def.createOrReplace(entity, val);
            }
        };
        componentsDefinition.set(componentId, newComponent);
        return newComponent;
    }
    function getComponent(componentId) {
        const component = componentsDefinition.get(componentId);
        if (!component) {
            throw new Error(`Component ${componentId} not found. You need to declare the components at the beginnig of the engine declaration`);
        }
        return component;
    }
    function getComponentOrNull(componentId) {
        return (componentsDefinition.get(componentId) ??
            /* istanbul ignore next */
            null);
    }
    function* getEntitiesWith(...components) {
        for (const [entity, ...groupComp] of getComponentDefGroup(...components)) {
            yield [entity, ...groupComp.map((c) => c.get(entity))];
        }
    }
    function* getComponentDefGroup(...args) {
        const [firstComponentDef, ...componentDefinitions] = args;
        for (const [entity] of firstComponentDef.iterator()) {
            let matches = true;
            for (const componentDef of componentDefinitions) {
                if (!componentDef.has(entity)) {
                    matches = false;
                    break;
                }
            }
            if (matches) {
                yield [entity, ...args];
            }
        }
    }
    function getSystems() {
        return systems.getSystems();
    }
    function componentsIter() {
        return componentsDefinition.values();
    }
    function removeComponentDefinition(componentId) {
        componentsDefinition.delete(componentId);
    }
    const Transform = Transform$1({ defineComponentFromSchema });
    function* getTreeEntityArray(firstEntity, proccesedEntities) {
        // This avoid infinite loop when there is a cyclic parenting
        if (proccesedEntities.find((value) => firstEntity === value))
            return;
        proccesedEntities.push(firstEntity);
        for (const [entity, value] of getEntitiesWith(Transform)) {
            if (value.parent === firstEntity) {
                yield* getTreeEntityArray(entity, proccesedEntities);
            }
        }
        yield firstEntity;
    }
    function removeEntityWithChildren(firstEntity) {
        for (const entity of getTreeEntityArray(firstEntity, [])) {
            removeEntity(entity);
        }
    }
    function seal() {
        if (!sealed) {
            sealed = true;
        }
    }
    return {
        addEntity,
        removeEntity,
        addSystem,
        getSystems,
        removeSystem,
        defineComponent,
        defineComponentFromSchema,
        defineValueSetComponentFromSchema,
        getEntitiesWith,
        getComponent,
        getComponentOrNull,
        removeComponentDefinition,
        removeEntityWithChildren,
        registerComponentDefinition,
        entityContainer,
        componentsIter,
        seal
    };
}
/**
 * @internal
 */
function Engine(options) {
    const partialEngine = preEngine();
    const crdtSystem = crdtSceneSystem(partialEngine, options?.onChangeFunction || null);
    async function update(dt) {
        await crdtSystem.receiveMessages();
        for (const system of partialEngine.getSystems()) {
            const ret = system.fn(dt);
            checkNotThenable(ret, `A system (${system.name || 'anonymous'}) returned a thenable. Systems cannot be async functions. Documentation: https://dcl.gg/sdk/sync-systems`);
        }
        // get the deleted entities to send the DeleteEntity CRDT commands
        const deletedEntites = partialEngine.entityContainer.releaseRemovedEntities();
        await crdtSystem.sendMessages(deletedEntites);
    }
    return {
        addEntity: partialEngine.addEntity,
        removeEntity: partialEngine.removeEntity,
        removeEntityWithChildren: partialEngine.removeEntityWithChildren,
        addSystem: partialEngine.addSystem,
        removeSystem: partialEngine.removeSystem,
        defineComponent: partialEngine.defineComponent,
        defineComponentFromSchema: partialEngine.defineComponentFromSchema,
        defineValueSetComponentFromSchema: partialEngine.defineValueSetComponentFromSchema,
        registerComponentDefinition: partialEngine.registerComponentDefinition,
        getEntitiesWith: partialEngine.getEntitiesWith,
        getComponent: partialEngine.getComponent,
        getComponentOrNull: partialEngine.getComponentOrNull,
        removeComponentDefinition: partialEngine.removeComponentDefinition,
        componentsIter: partialEngine.componentsIter,
        seal: partialEngine.seal,
        update,
        RootEntity: 0,
        PlayerEntity: 1,
        CameraEntity: 2,
        getEntityState: partialEngine.entityContainer.getEntityState,
        addTransport: crdtSystem.addTransport,
        entityContainer: partialEngine.entityContainer
    };
}

function getAndClean(value) {
    const messagesToProcess = Array.from(value);
    value.length = 0;
    return messagesToProcess;
}
/**
 * @internal
 */
function createTaskSystem(engine) {
    const tasks = [];
    async function runTask(task) {
        try {
            const resp = await task();
            return resp;
        }
        catch (e) {
            console.error(e);
        }
    }
    function executeTasks() {
        for (const task of getAndClean(tasks)) {
            runTask(task).catch(console.error);
        }
    }
    engine.addSystem(executeTasks);
    return {
        executeTask(task) {
            tasks.push(task);
        }
    };
}

/**
 * @internal
 */
function createPointerEventSystem(engine, inputSystem) {
    const PointerEvents = PointerEvents$1(engine);
    let EventType;
    (function (EventType) {
        EventType[EventType["Click"] = 0] = "Click";
        EventType[EventType["Down"] = 1] = "Down";
        EventType[EventType["Up"] = 2] = "Up";
    })(EventType || (EventType = {}));
    const getDefaultOpts = (opts = {}) => ({
        button: 3 /* InputAction.IA_ANY */,
        ...opts
    });
    const eventsMap = new Map();
    function getEvent(entity) {
        return eventsMap.get(entity) || eventsMap.set(entity, new Map()).get(entity);
    }
    function setPointerEvent(entity, type, opts) {
        if (opts.hoverText || opts.showFeedback) {
            const pointerEvent = PointerEvents.getMutableOrNull(entity) || PointerEvents.create(entity);
            pointerEvent.pointerEvents.push({
                eventType: type,
                eventInfo: {
                    button: opts.button,
                    showFeedback: opts.showFeedback,
                    hoverText: opts.hoverText,
                    maxDistance: opts.maxDistance
                }
            });
        }
    }
    function removePointerEvent(entity, type, button) {
        const pointerEvent = PointerEvents.getMutableOrNull(entity);
        if (!pointerEvent)
            return;
        pointerEvent.pointerEvents = pointerEvent.pointerEvents.filter((pointer) => !(pointer.eventInfo?.button === button && pointer.eventType === type));
    }
    function getPointerEvent(eventType) {
        if (eventType === EventType.Up) {
            return 0 /* PointerEventType.PET_UP */;
        }
        return 1 /* PointerEventType.PET_DOWN */;
    }
    function removeEvent(entity, type) {
        const event = getEvent(entity);
        const pointerEvent = event.get(type);
        if (pointerEvent?.opts.hoverText) {
            removePointerEvent(entity, getPointerEvent(type), pointerEvent.opts.button);
        }
        event.delete(type);
    }
    // @internal
    engine.addSystem(function EventSystem() {
        for (const [entity, event] of eventsMap) {
            if (engine.getEntityState(entity) === EntityState.Removed) {
                eventsMap.delete(entity);
                continue;
            }
            for (const [eventType, { cb, opts }] of event) {
                if (eventType === EventType.Click) {
                    const command = inputSystem.getClick(opts.button, entity);
                    if (command)
                        checkNotThenable(cb(command.up), 'Click event returned a thenable. Only synchronous functions are allowed');
                }
                if (eventType === EventType.Down || eventType === EventType.Up) {
                    const command = inputSystem.getInputCommand(opts.button, getPointerEvent(eventType), entity);
                    if (command) {
                        checkNotThenable(cb(command), 'Event handler returned a thenable. Only synchronous functions are allowed');
                    }
                }
            }
        }
    });
    return {
        removeOnClick(entity) {
            removeEvent(entity, EventType.Click);
        },
        removeOnPointerDown(entity) {
            removeEvent(entity, EventType.Down);
        },
        removeOnPointerUp(entity) {
            removeEvent(entity, EventType.Up);
        },
        onClick(entity, cb, opts) {
            const options = getDefaultOpts(opts);
            // Clear previous event with over feedback included
            removeEvent(entity, EventType.Click);
            // Set new event
            getEvent(entity).set(EventType.Click, { cb, opts: options });
            setPointerEvent(entity, 1 /* PointerEventType.PET_DOWN */, options);
        },
        onPointerDown(entity, cb, opts) {
            const options = getDefaultOpts(opts);
            removeEvent(entity, EventType.Down);
            getEvent(entity).set(EventType.Down, { cb, opts: options });
            setPointerEvent(entity, 1 /* PointerEventType.PET_DOWN */, options);
        },
        onPointerUp(entity, cb, opts) {
            const options = getDefaultOpts(opts);
            removeEvent(entity, EventType.Up);
            getEvent(entity).set(EventType.Up, { cb, opts: options });
            setPointerEvent(entity, 0 /* PointerEventType.PET_UP */, options);
        }
    };
}

/**
 * @alpha * This file initialization is an alpha one. This is based on the old-ecs
 * init and it'll be changing.
 */
/**
 * @public
 * The engine is the part of the scene that sits in the middle and manages all of the other parts.
 * It determines what entities are rendered and how players interact with them.
 * It also coordinates what functions from systems are executed and when.
 *
 * @example
 * import { engine } from '@dcl/sdk/ecs'
 * const entity = engine.addEntity()
 * engine.addSystem(someSystemFunction)
 *
 */

const engine = Engine();
/**
 * @public
 * Input system manager. Check for button events
 * @example
 * inputSystem.isTriggered: Returns true if an input action ocurred since the last tick.
 * inputSystem.isPressed: Returns true if an input is currently being pressed down. It will return true on every tick until the button goes up again.
 * inputSystem.getInputCommand: Returns an object with data about the input action.
 */

const inputSystem = createInputSystem(engine);
/**
 * @public
 * Register callback functions to a particular entity.
 */

createPointerEventSystem(engine, inputSystem);
/**
 * @public
 * Runs an async function
 */

const executeTask = createTaskSystem(engine).executeTask;

/** @public */  AudioSource(engine);
/** @public */  AudioStream(engine);
/** @public */  AvatarAttach(engine);
/** @public */  AvatarModifierArea(engine);
/** @public */  AvatarShape(engine);
/** @public */  Billboard(engine);
/** @public */  CameraMode(engine);
/** @public */  CameraModeArea(engine);
/** @public */  GltfContainer(engine);
/** @public */  NftShape(engine);
/** @public */  const PointerEvents = PointerEvents$1(engine);
/** @public */  PointerEventsResult(engine);
/** @public */  PointerLock(engine);
/** @public */  Raycast(engine);
/** @public */  RaycastResult(engine);
/** @public */  TextShape(engine);
/** @public */  UiBackground(engine);
/** @public */  UiDropdown(engine);
/** @public */  UiDropdownResult(engine);
/** @public */  UiInput(engine);
/** @public */  UiInputResult(engine);
/** @public */  UiText(engine);
/** @public */  UiTransform(engine);
/** @public */  VideoPlayer(engine);
/** @public */  VisibilityComponent(engine);

// The order of the following imports matters. Please do not auto-sort
// export components for global engine
 const Transform = Transform$1(engine);
 Animator(engine);
 const Material = Material$1(engine);
 const MeshRenderer = MeshRenderer$1(engine);
 const MeshCollider = MeshCollider$1(engine);

/**
 * Constant used to convert a value to gamma space
 * @public
 */
const ToGammaSpace = 1 / 2.2;
/**
 * Constant used to convert a value to linear space
 * @public
 */
const ToLinearSpace = 2.2;
/**
 * Constant used to define the minimal number value in Babylon.js
 * @public
 */
const Epsilon = 0.000001;
/**
 * Constant used to convert from Euler degrees to radians
 * @public
 */
const DEG2RAD = Math.PI / 180;
/**
 * Constant used to convert from radians to Euler degrees
 * @public
 */
const RAD2DEG = 360 / (Math.PI * 2);

/**
 * Scalar computation library
 * @public
 */
var Scalar;
(function (Scalar) {
    /**
     * Two pi constants convenient for computation.
     */
    Scalar.TwoPi = Math.PI * 2;
    /**
     * Boolean : true if the absolute difference between a and b is lower than epsilon (default = 1.401298E-45)
     * @param a - number
     * @param b - number
     * @param epsilon - (default = 1.401298E-45)
     * @returns true if the absolute difference between a and b is lower than epsilon (default = 1.401298E-45)
     */
    function withinEpsilon(a, b, epsilon = 1.401298e-45) {
        const num = a - b;
        return -epsilon <= num && num <= epsilon;
    }
    Scalar.withinEpsilon = withinEpsilon;
    /**
     * Returns a string : the upper case translation of the number i to hexadecimal.
     * @param i - number
     * @returns the upper case translation of the number i to hexadecimal.
     */
    function toHex(i) {
        const str = i.toString(16);
        if (i <= 15) {
            return ('0' + str).toUpperCase();
        }
        return str.toUpperCase();
    }
    Scalar.toHex = toHex;
    /**
     * Returns -1 if value is negative and +1 is value is positive.
     * @param _value - the value
     * @returns the value itself if it's equal to zero.
     */
    function sign(value) {
        const _value = +value; // convert to a number
        if (_value === 0 || isNaN(_value)) {
            return _value;
        }
        return _value > 0 ? 1 : -1;
    }
    Scalar.sign = sign;
    /**
     * Returns the value itself if it's between min and max.
     * Returns min if the value is lower than min.
     * Returns max if the value is greater than max.
     * @param value - the value to clmap
     * @param min - the min value to clamp to (default: 0)
     * @param max - the max value to clamp to (default: 1)
     * @returns the clamped value
     */
    function clamp(value, min = 0, max = 1) {
        return Math.min(max, Math.max(min, value));
    }
    Scalar.clamp = clamp;
    /**
     * the log2 of value.
     * @param value - the value to compute log2 of
     * @returns the log2 of value.
     */
    function log2(value) {
        return Math.log(value) * Math.LOG2E;
    }
    Scalar.log2 = log2;
    /**
     * Loops the value, so that it is never larger than length and never smaller than 0.
     *
     * This is similar to the modulo operator but it works with floating point numbers.
     * For example, using 3.0 for t and 2.5 for length, the result would be 0.5.
     * With t = 5 and length = 2.5, the result would be 0.0.
     * Note, however, that the behaviour is not defined for negative numbers as it is for the modulo operator
     * @param value - the value
     * @param length - the length
     * @returns the looped value
     */
    function repeat(value, length) {
        return value - Math.floor(value / length) * length;
    }
    Scalar.repeat = repeat;
    /**
     * Normalize the value between 0.0 and 1.0 using min and max values
     * @param value - value to normalize
     * @param min - max to normalize between
     * @param max - min to normalize between
     * @returns the normalized value
     */
    function normalize(value, min, max) {
        return (value - min) / (max - min);
    }
    Scalar.normalize = normalize;
    /**
     * Denormalize the value from 0.0 and 1.0 using min and max values
     * @param normalized - value to denormalize
     * @param min - max to denormalize between
     * @param max - min to denormalize between
     * @returns the denormalized value
     */
    function denormalize(normalized, min, max) {
        return normalized * (max - min) + min;
    }
    Scalar.denormalize = denormalize;
    /**
     * Calculates the shortest difference between two given angles given in degrees.
     * @param current - current angle in degrees
     * @param target - target angle in degrees
     * @returns the delta
     */
    function deltaAngle(current, target) {
        let num = repeat(target - current, 360.0);
        if (num > 180.0) {
            num -= 360.0;
        }
        return num;
    }
    Scalar.deltaAngle = deltaAngle;
    /**
     * PingPongs the value t, so that it is never larger than length and never smaller than 0.
     * @param tx - value
     * @param length - length
     * @returns The returned value will move back and forth between 0 and length
     */
    function pingPong(tx, length) {
        const t = repeat(tx, length * 2.0);
        return length - Math.abs(t - length);
    }
    Scalar.pingPong = pingPong;
    /**
     * Interpolates between min and max with smoothing at the limits.
     *
     * This export function interpolates between min and max in a similar way to Lerp. However, the interpolation will gradually speed up
     * from the start and slow down toward the end. This is useful for creating natural-looking animation, fading and other transitions.
     * @param from - from
     * @param to - to
     * @param tx - value
     * @returns the smooth stepped value
     */
    function smoothStep(from, to, tx) {
        let t = clamp(tx);
        t = -2.0 * t * t * t + 3.0 * t * t;
        return to * t + from * (1.0 - t);
    }
    Scalar.smoothStep = smoothStep;
    /**
     * Moves a value current towards target.
     *
     * This is essentially the same as Mathf.Lerp but instead the export function will ensure that the speed never exceeds maxDelta.
     * Negative values of maxDelta pushes the value away from target.
     * @param current - current value
     * @param target - target value
     * @param maxDelta - max distance to move
     * @returns resulting value
     */
    function moveTowards(current, target, maxDelta) {
        let result = 0;
        if (Math.abs(target - current) <= maxDelta) {
            result = target;
        }
        else {
            result = current + sign(target - current) * maxDelta;
        }
        return result;
    }
    Scalar.moveTowards = moveTowards;
    /**
     * Same as MoveTowards but makes sure the values interpolate correctly when they wrap around 360 degrees.
     *
     * Variables current and target are assumed to be in degrees. For optimization reasons, negative values of maxDelta
     *  are not supported and may cause oscillation. To push current away from a target angle, add 180 to that angle instead.
     * @param current - current value
     * @param target - target value
     * @param maxDelta - max distance to move
     * @returns resulting angle
     */
    function moveTowardsAngle(current, target, maxDelta) {
        const num = deltaAngle(current, target);
        let result = 0;
        if (-maxDelta < num && num < maxDelta) {
            result = target;
        }
        else {
            result = moveTowards(current, current + num, maxDelta);
        }
        return result;
    }
    Scalar.moveTowardsAngle = moveTowardsAngle;
    /**
     * Creates a new scalar with values linearly interpolated of "amount" between the start scalar and the end scalar
     * @param start - start value
     * @param end - target value
     * @param amount - amount to lerp between
     * @returns the lerped value
     */
    function lerp(start, end, amount) {
        return start + (end - start) * amount;
    }
    Scalar.lerp = lerp;
    /**
     * Same as Lerp but makes sure the values interpolate correctly when they wrap around 360 degrees.
     * The parameter t is clamped to the range [0, 1]. Variables a and b are assumed to be in degrees.
     * @param start - start value
     * @param end - target value
     * @param amount - amount to lerp between
     * @returns the lerped value
     */
    function lerpAngle(start, end, amount) {
        let num = repeat(end - start, 360.0);
        if (num > 180.0) {
            num -= 360.0;
        }
        return start + num * clamp(amount);
    }
    Scalar.lerpAngle = lerpAngle;
    /**
     * Calculates the linear parameter t that produces the interpolant value within the range [a, b].
     * @param a - start value
     * @param b - target value
     * @param value - value between a and b
     * @returns the inverseLerp value
     */
    function inverseLerp(a, b, value) {
        let result = 0;
        if (a !== b) {
            result = clamp((value - a) / (b - a));
        }
        else {
            result = 0.0;
        }
        return result;
    }
    Scalar.inverseLerp = inverseLerp;
    /**
     * Returns a new scalar located for "amount" (float) on the Hermite spline defined by the scalars "value1", "value3", "tangent1", "tangent2".
     * {@link http://mathworld.wolfram.com/HermitePolynomial.html}
     * @param value1 - spline value
     * @param tangent1 - spline value
     * @param value2 - spline value
     * @param tangent2 - spline value
     * @param amount - input value
     * @returns hermite result
     */
    function hermite(value1, tangent1, value2, tangent2, amount) {
        const squared = amount * amount;
        const cubed = amount * squared;
        const part1 = 2.0 * cubed - 3.0 * squared + 1.0;
        const part2 = -2.0 * cubed + 3.0 * squared;
        const part3 = cubed - 2.0 * squared + amount;
        const part4 = cubed - squared;
        return value1 * part1 + value2 * part2 + tangent1 * part3 + tangent2 * part4;
    }
    Scalar.hermite = hermite;
    /**
     * Returns a random float number between and min and max values
     * @param min - min value of random
     * @param max - max value of random
     * @returns random value
     */
    function randomRange(min, max) {
        if (min === max) {
            return min;
        }
        return Math.random() * (max - min) + min;
    }
    Scalar.randomRange = randomRange;
    /**
     * This export function returns percentage of a number in a given range.
     *
     * RangeToPercent(40,20,60) will return 0.5 (50%)
     * RangeToPercent(34,0,100) will return 0.34 (34%)
     * @param num - to convert to percentage
     * @param min - min range
     * @param max - max range
     * @returns the percentage
     */
    function rangeToPercent(num, min, max) {
        return (num - min) / (max - min);
    }
    Scalar.rangeToPercent = rangeToPercent;
    /**
     * This export function returns number that corresponds to the percentage in a given range.
     *
     * PercentToRange(0.34,0,100) will return 34.
     * @param percent - to convert to number
     * @param min - min range
     * @param max - max range
     * @returns the number
     */
    function percentToRange(percent, min, max) {
        return (max - min) * percent + min;
    }
    Scalar.percentToRange = percentToRange;
    /**
     * Returns the angle converted to equivalent value between -Math.PI and Math.PI radians.
     * @param angle - The angle to normalize in radian.
     * @returns The converted angle.
     */
    function normalizeRadians(angle) {
        // More precise but slower version kept for reference.
        // tslint:disable:no-commented-out-code
        /*
        // angle = angle % Tools.TwoPi;
        // angle = (angle + Tools.TwoPi) % Tools.TwoPi;
    
        //if (angle > Math.PI) {
        //	angle -= Tools.TwoPi;
        //}
          */
        return angle - Scalar.TwoPi * Math.floor((angle + Math.PI) / Scalar.TwoPi);
    }
    Scalar.normalizeRadians = normalizeRadians;
})(Scalar || (Scalar = {}));

/**
 * @public
 * Vector3 is a type and a namespace.
 * ```
 * // The namespace contains all types and functions to operates with Vector3
 * const next = Vector3.add(pointA, velocityA)
 * // The type Vector3 is an alias to Vector3.ReadonlyVector3
 * const readonlyPosition: Vector3 = Vector3.Zero()
 * readonlyPosition.x = 0.1 // this FAILS
 *
 * // For mutable usage, use `Vector3.Mutable`
 * const position: Vector3.Mutable = Vector3.One()
 * position.x = 3.0 // this WORKS
 * ```
 */
var Vector3;
(function (Vector3) {
    /**
     * Gets a boolean indicating that the vector is non uniform meaning x, y or z are not all the same
     * @param vector - vector to check
     */
    function isNonUniform(vector) {
        const absX = Math.abs(vector.x);
        const absY = Math.abs(vector.y);
        if (absX !== absY) {
            return true;
        }
        const absZ = Math.abs(vector.z);
        if (absX !== absZ) {
            return true;
        }
        return false;
    }
    Vector3.isNonUniform = isNonUniform;
    /**
     * Creates a new Vector3 object from the given x, y, z (floats) coordinates.
     * @param x - defines the first coordinates (on X axis)
     * @param y - defines the second coordinates (on Y axis)
     * @param z - defines the third coordinates (on Z axis)
     */
    function create(
    /**
     * Defines the first coordinates (on X axis)
     */
    x = 0, 
    /**
     * Defines the second coordinates (on Y axis)
     */
    y = 0, 
    /**
     * Defines the third coordinates (on Z axis)
     */
    z = 0) {
        return { x, y, z };
    }
    Vector3.create = create;
    /**
     * Returns a new Vector3 as the result of the addition of the two given vectors.
     * @param vector1 - the first vector
     * @param vector2 - the second vector
     * @returns the resulting vector
     */
    function add(vector1, vector2) {
        return {
            x: vector1.x + vector2.x,
            y: vector1.y + vector2.y,
            z: vector1.z + vector2.z
        };
    }
    Vector3.add = add;
    /**
     * Add component by component the vector2 into dest
     * @param dest - the first vector and destination of addition
     * @param vector2 - the second vector
     */
    function addToRef(vector1, vector2, result) {
        result.x = vector1.x + vector2.x;
        result.y = vector1.y + vector2.y;
        result.z = vector1.z + vector2.z;
    }
    Vector3.addToRef = addToRef;
    /**
     * Returns a new Vector3 as the result of the substraction of the two given vectors.
     * @returns the resulting vector
     */
    function subtract(vector1, vector2) {
        return {
            x: vector1.x - vector2.x,
            y: vector1.y - vector2.y,
            z: vector1.z - vector2.z
        };
    }
    Vector3.subtract = subtract;
    /**
     * Returns a new Vector3 as the result of the substraction of the two given vectors.
     * @returns the resulting vector
     */
    function subtractToRef(vector1, vector2, result) {
        result.x = vector1.x - vector2.x;
        result.y = vector1.y - vector2.y;
        result.z = vector1.z - vector2.z;
    }
    Vector3.subtractToRef = subtractToRef;
    /**
     * Subtracts the given floats from the current Vector3 coordinates and set the given vector "result" with this result
     * @param x - defines the x coordinate of the operand
     * @param y - defines the y coordinate of the operand
     * @param z - defines the z coordinate of the operand
     * @param result - defines the Vector3 object where to store the result
     */
    function subtractFromFloatsToRef(vector1, x, y, z, result) {
        result.x = vector1.x - x;
        result.y = vector1.y - y;
        result.z = vector1.z - z;
    }
    Vector3.subtractFromFloatsToRef = subtractFromFloatsToRef;
    /**
     * Returns a new Vector3 with the other sign
     * @returns the resulting vector
     */
    function negate(value) {
        return { x: -value.x, y: -value.y, z: -value.z };
    }
    Vector3.negate = negate;
    /**
     * Copy source into dest
     *
     */
    function copyFrom(source, dest) {
        dest.x = source.x;
        dest.y = source.y;
        dest.z = source.z;
    }
    Vector3.copyFrom = copyFrom;
    /**
     * Sets the given vector "dest" with the given floats.
     * @param x - defines the x coordinate of the source
     * @param y - defines the y coordinate of the source
     * @param z - defines the z coordinate of the source
     * @param dest - defines the Vector3 where to store the result
     */
    function copyFromFloats(x, y, z, dest) {
        dest.x = x;
        dest.y = y;
        dest.z = z;
    }
    Vector3.copyFromFloats = copyFromFloats;
    /**
     * Returns a new Vector3 with the same value
     * @returns the resulting vector
     */
    function clone(source) {
        return create(source.x, source.y, source.z);
    }
    Vector3.clone = clone;
    /**
     * Get the clip factor between two vectors
     * @param vector0 - defines the first operand
     * @param vector1 - defines the second operand
     * @param axis - defines the axis to use
     * @param size - defines the size along the axis
     * @returns the clip factor
     */
    function getClipFactor(vector0, vector1, axis, size) {
        const d0 = dot(vector0, axis) - size;
        const d1 = dot(vector1, axis) - size;
        const s = d0 / (d0 - d1);
        return s;
    }
    Vector3.getClipFactor = getClipFactor;
    /**
     * Get angle between two vectors
     * @param vector0 - angle between vector0 and vector1
     * @param vector1 - angle between vector0 and vector1
     * @param normal - direction of the normal
     * @returns the angle between vector0 and vector1
     */
    function getAngleBetweenVectors(vector0, vector1, normal) {
        const v0 = normalize(vector0);
        const v1 = normalize(vector1);
        const v0v1dot = dot(v0, v1);
        const n = create();
        crossToRef(v0, v1, n);
        if (dot(n, normal) > 0) {
            return Math.acos(v0v1dot);
        }
        return -Math.acos(v0v1dot);
    }
    Vector3.getAngleBetweenVectors = getAngleBetweenVectors;
    /**
     * Returns a new Vector3 set from the index "offset" of the given array
     * @param array - defines the source array
     * @param offset - defines the offset in the source array
     * @returns the new Vector3
     */
    function fromArray(array, offset = 0) {
        return create(array[offset], array[offset + 1], array[offset + 2]);
    }
    Vector3.fromArray = fromArray;
    /**
     * Returns a new Vector3 set from the index "offset" of the given FloatArray
     * This function is deprecated.  Use FromArray instead
     * @param array - defines the source array
     * @param offset - defines the offset in the source array
     * @returns the new Vector3
     */
    function fromFloatArray(array, offset) {
        return fromArray(array, offset);
    }
    Vector3.fromFloatArray = fromFloatArray;
    /**
     * Sets the given vector "result" with the element values from the index "offset" of the given array
     * @param array - defines the source array
     * @param offset - defines the offset in the source array
     * @param result - defines the Vector3 where to store the result
     */
    function fromArrayToRef(array, offset, result) {
        result.x = array[offset];
        result.y = array[offset + 1];
        result.z = array[offset + 2];
    }
    Vector3.fromArrayToRef = fromArrayToRef;
    /**
     * Sets the given vector "result" with the element values from the index "offset" of the given FloatArray
     * This function is deprecated.  Use FromArrayToRef instead.
     * @param array - defines the source array
     * @param offset - defines the offset in the source array
     * @param result - defines the Vector3 where to store the result
     */
    function fromFloatArrayToRef(array, offset, result) {
        return fromArrayToRef(array, offset, result);
    }
    Vector3.fromFloatArrayToRef = fromFloatArrayToRef;
    // Properties
    /**
     * Gets the length of the Vector3
     * @returns the length of the Vector3
     */
    function length(vector) {
        return Math.sqrt(vector.x * vector.x + vector.y * vector.y + vector.z * vector.z);
    }
    Vector3.length = length;
    /**
     * Gets the squared length of the Vector3
     * @returns squared length of the Vector3
     */
    function lengthSquared(vector) {
        return vector.x * vector.x + vector.y * vector.y + vector.z * vector.z;
    }
    Vector3.lengthSquared = lengthSquared;
    /**
     * Returns a new Vector3 set with the current Vector3 coordinates multiplied by the float "scale"
     * @param scale - defines the multiplier factor
     * @returns a new Vector3
     */
    function scaleToRef(vector, scale, result) {
        result.x = vector.x * scale;
        result.y = vector.y * scale;
        result.z = vector.z * scale;
    }
    Vector3.scaleToRef = scaleToRef;
    /**
     * Returns a new Vector3 set with the current Vector3 coordinates multiplied by the float "scale"
     * @param scale - defines the multiplier factor
     * @returns a new Vector3
     */
    function scale(vector, scale) {
        return create(vector.x * scale, vector.y * scale, vector.z * scale);
    }
    Vector3.scale = scale;
    /**
     * Normalize the current Vector3 with the given input length.
     * Please note that this is an in place operation.
     * @param len - the length of the vector
     * @returns the current updated Vector3
     */
    function normalizeFromLength(vector, len) {
        const result = create(0, 0, 0);
        normalizeFromLengthToRef(vector, len, result);
        return result;
    }
    Vector3.normalizeFromLength = normalizeFromLength;
    /**
     * Normalize the current Vector3 with the given input length.
     * Please note that this is an in place operation.
     * @param len - the length of the vector
     * @returns the current updated Vector3
     */
    function normalizeFromLengthToRef(vector, len, result) {
        if (len === 0 || len === 1.0) {
            copyFrom(vector, result);
            return;
        }
        scaleToRef(vector, 1.0 / len, result);
    }
    Vector3.normalizeFromLengthToRef = normalizeFromLengthToRef;
    /**
     * Normalize the current Vector3.
     * Please note that this is an in place operation.
     * @returns the current updated Vector3
     */
    function normalize(vector) {
        return normalizeFromLength(vector, length(vector));
    }
    Vector3.normalize = normalize;
    /**
     * Normalize the current Vector3.
     * Please note that this is an in place operation.
     * @returns the current updated Vector3
     */
    function normalizeToRef(vector, result) {
        normalizeFromLengthToRef(vector, length(vector), result);
    }
    Vector3.normalizeToRef = normalizeToRef;
    /**
     * Returns the dot product (float) between the vectors "left" and "right"
     * @param left - defines the left operand
     * @param right - defines the right operand
     * @returns the dot product
     */
    function dot(left, right) {
        return left.x * right.x + left.y * right.y + left.z * right.z;
    }
    Vector3.dot = dot;
    /**
     * Multiplies this vector (with an implicit 1 in the 4th dimension) and m, and divides by perspective
     * @param matrix - The transformation matrix
     * @returns result Vector3
     */
    function applyMatrix4(vector, matrix) {
        const result = clone(vector);
        applyMatrix4ToRef(vector, matrix, result);
        return result;
    }
    Vector3.applyMatrix4 = applyMatrix4;
    /**
     * Multiplies this vector (with an implicit 1 in the 4th dimension) and m, and divides by perspective and set the given vector "result" with this result
     * @param matrix - The transformation matrix
     * @param result - defines the Vector3 object where to store the result
     */
    function applyMatrix4ToRef(vector, matrix, result) {
        const { x, y, z } = vector;
        const m = matrix._m;
        const w = 1 / (m[3] * x + m[7] * y + m[11] * z + m[15]);
        result.x = (m[0] * x + m[4] * y + m[8] * z + m[12]) * w;
        result.y = (m[1] * x + m[5] * y + m[9] * z + m[13]) * w;
        result.z = (m[2] * x + m[6] * y + m[10] * z + m[14]) * w;
    }
    Vector3.applyMatrix4ToRef = applyMatrix4ToRef;
    /**
     * Rotates the current Vector3 based on the given quaternion
     * @param q - defines the Quaternion
     * @returns the current Vector3
     */
    function rotate(vector, q) {
        const result = create();
        rotateToRef(vector, q, result);
        return result;
    }
    Vector3.rotate = rotate;
    /**
     * Rotates current Vector3 based on the given quaternion, but applies the rotation to target Vector3.
     * @param q - defines the Quaternion
     * @param result - defines the target Vector3
     * @returns the current Vector3
     */
    function rotateToRef(vector, q, result) {
        const { x, y, z } = vector;
        const { x: qx, y: qy, z: qz, w: qw } = q;
        // calculate quat * vector
        const ix = qw * x + qy * z - qz * y;
        const iy = qw * y + qz * x - qx * z;
        const iz = qw * z + qx * y - qy * x;
        const iw = -qx * x - qy * y - qz * z;
        // calculate result * inverse quat
        result.x = ix * qw + iw * -qx + iy * -qz - iz * -qy;
        result.y = iy * qw + iw * -qy + iz * -qx - ix * -qz;
        result.z = iz * qw + iw * -qz + ix * -qy - iy * -qx;
    }
    Vector3.rotateToRef = rotateToRef;
    /**
     * Returns a new Vector3 located for "amount" (float) on the linear interpolation between the vectors "start" and "end"
     * @param start - defines the start value
     * @param end - defines the end value
     * @param amount - max defines amount between both (between 0 and 1)
     * @returns the new Vector3
     */
    function lerp(start, end, amount) {
        const result = create(0, 0, 0);
        lerpToRef(start, end, amount, result);
        return result;
    }
    Vector3.lerp = lerp;
    /**
     * Sets the given vector "result" with the result of the linear interpolation from the vector "start" for "amount" to the vector "end"
     * @param start - defines the start value
     * @param end - defines the end value
     * @param amount - max defines amount between both (between 0 and 1)
     * @param result - defines the Vector3 where to store the result
     */
    function lerpToRef(start, end, amount, result) {
        result.x = start.x + (end.x - start.x) * amount;
        result.y = start.y + (end.y - start.y) * amount;
        result.z = start.z + (end.z - start.z) * amount;
    }
    Vector3.lerpToRef = lerpToRef;
    /**
     * Returns a new Vector3 as the cross product of the vectors "left" and "right"
     * The cross product is then orthogonal to both "left" and "right"
     * @param left - defines the left operand
     * @param right - defines the right operand
     * @returns the cross product
     */
    function cross(left, right) {
        const result = Zero();
        crossToRef(left, right, result);
        return result;
    }
    Vector3.cross = cross;
    /**
     * Sets the given vector "result" with the cross product of "left" and "right"
     * The cross product is then orthogonal to both "left" and "right"
     * @param left - defines the left operand
     * @param right - defines the right operand
     * @param result - defines the Vector3 where to store the result
     */
    function crossToRef(left, right, result) {
        result.x = left.y * right.z - left.z * right.y;
        result.y = left.z * right.x - left.x * right.z;
        result.z = left.x * right.y - left.y * right.x;
    }
    Vector3.crossToRef = crossToRef;
    /**
     * Returns a new Vector3 set with the result of the transformation by the given matrix of the given vector.
     * This method computes tranformed coordinates only, not transformed direction vectors (ie. it takes translation in account)
     * @param vector - defines the Vector3 to transform
     * @param transformation - defines the transformation matrix
     * @returns the transformed Vector3
     */
    function transformCoordinates(vector, transformation) {
        const result = Zero();
        transformCoordinatesToRef(vector, transformation, result);
        return result;
    }
    Vector3.transformCoordinates = transformCoordinates;
    /**
     * Sets the given vector "result" coordinates with the result of the transformation by the given matrix of the given vector
     * This method computes tranformed coordinates only, not transformed direction vectors (ie. it takes translation in account)
     * @param vector - defines the Vector3 to transform
     * @param transformation - defines the transformation matrix
     * @param result - defines the Vector3 where to store the result
     */
    function transformCoordinatesToRef(vector, transformation, result) {
        return transformCoordinatesFromFloatsToRef(vector.x, vector.y, vector.z, transformation, result);
    }
    Vector3.transformCoordinatesToRef = transformCoordinatesToRef;
    /**
     * Sets the given vector "result" coordinates with the result of the transformation by the given matrix of the given floats (x, y, z)
     * This method computes tranformed coordinates only, not transformed direction vectors
     * @param x - define the x coordinate of the source vector
     * @param y - define the y coordinate of the source vector
     * @param z - define the z coordinate of the source vector
     * @param transformation - defines the transformation matrix
     * @param result - defines the Vector3 where to store the result
     */
    function transformCoordinatesFromFloatsToRef(x, y, z, transformation, result) {
        const m = transformation._m;
        const rx = x * m[0] + y * m[4] + z * m[8] + m[12];
        const ry = x * m[1] + y * m[5] + z * m[9] + m[13];
        const rz = x * m[2] + y * m[6] + z * m[10] + m[14];
        const rw = 1 / (x * m[3] + y * m[7] + z * m[11] + m[15]);
        result.x = rx * rw;
        result.y = ry * rw;
        result.z = rz * rw;
    }
    Vector3.transformCoordinatesFromFloatsToRef = transformCoordinatesFromFloatsToRef;
    /**
     * Returns a new Vector3 set with the result of the normal transformation by the given matrix of the given vector
     * This methods computes transformed normalized direction vectors only (ie. it does not apply translation)
     * @param vector - defines the Vector3 to transform
     * @param transformation - defines the transformation matrix
     * @returns the new Vector3
     */
    function transformNormal(vector, transformation) {
        const result = Zero();
        transformNormalToRef(vector, transformation, result);
        return result;
    }
    Vector3.transformNormal = transformNormal;
    /**
     * Sets the given vector "result" with the result of the normal transformation by the given matrix of the given vector
     * This methods computes transformed normalized direction vectors only (ie. it does not apply translation)
     * @param vector - defines the Vector3 to transform
     * @param transformation - defines the transformation matrix
     * @param result - defines the Vector3 where to store the result
     */
    function transformNormalToRef(vector, transformation, result) {
        transformNormalFromFloatsToRef(vector.x, vector.y, vector.z, transformation, result);
    }
    Vector3.transformNormalToRef = transformNormalToRef;
    /**
     * Sets the given vector "result" with the result of the normal transformation by the given matrix of the given floats (x, y, z)
     * This methods computes transformed normalized direction vectors only (ie. it does not apply translation)
     * @param x - define the x coordinate of the source vector
     * @param y - define the y coordinate of the source vector
     * @param z - define the z coordinate of the source vector
     * @param transformation - defines the transformation matrix
     * @param result - defines the Vector3 where to store the result
     */
    function transformNormalFromFloatsToRef(x, y, z, transformation, result) {
        const m = transformation._m;
        result.x = x * m[0] + y * m[4] + z * m[8];
        result.y = x * m[1] + y * m[5] + z * m[9];
        result.z = x * m[2] + y * m[6] + z * m[10];
    }
    Vector3.transformNormalFromFloatsToRef = transformNormalFromFloatsToRef;
    /**
     * Returns a new Vector3 located for "amount" on the CatmullRom interpolation spline defined by the vectors "value1", "value2", "value3", "value4"
     * @param value1 - defines the first control point
     * @param value2 - defines the second control point
     * @param value3 - defines the third control point
     * @param value4 - defines the fourth control point
     * @param amount - defines the amount on the spline to use
     * @returns the new Vector3
     */
    function catmullRom(value1, value2, value3, value4, amount) {
        const squared = amount * amount;
        const cubed = amount * squared;
        const x = 0.5 *
            (2.0 * value2.x +
                (-value1.x + value3.x) * amount +
                (2.0 * value1.x - 5.0 * value2.x + 4.0 * value3.x - value4.x) *
                    squared +
                (-value1.x + 3.0 * value2.x - 3.0 * value3.x + value4.x) * cubed);
        const y = 0.5 *
            (2.0 * value2.y +
                (-value1.y + value3.y) * amount +
                (2.0 * value1.y - 5.0 * value2.y + 4.0 * value3.y - value4.y) *
                    squared +
                (-value1.y + 3.0 * value2.y - 3.0 * value3.y + value4.y) * cubed);
        const z = 0.5 *
            (2.0 * value2.z +
                (-value1.z + value3.z) * amount +
                (2.0 * value1.z - 5.0 * value2.z + 4.0 * value3.z - value4.z) *
                    squared +
                (-value1.z + 3.0 * value2.z - 3.0 * value3.z + value4.z) * cubed);
        return create(x, y, z);
    }
    Vector3.catmullRom = catmullRom;
    /**
     * Returns a new Vector3 set with the coordinates of "value", if the vector "value" is in the cube defined by the vectors "min" and "max"
     * If a coordinate value of "value" is lower than one of the "min" coordinate, then this "value" coordinate is set with the "min" one
     * If a coordinate value of "value" is greater than one of the "max" coordinate, then this "value" coordinate is set with the "max" one
     * @param value - defines the current value
     * @param min - defines the lower range value
     * @param max - defines the upper range value
     * @returns the new Vector3
     */
    function clamp(value, min, max) {
        const v = create();
        clampToRef(value, min, max, v);
        return v;
    }
    Vector3.clamp = clamp;
    /**
     * Sets the given vector "result" with the coordinates of "value", if the vector "value" is in the cube defined by the vectors "min" and "max"
     * If a coordinate value of "value" is lower than one of the "min" coordinate, then this "value" coordinate is set with the "min" one
     * If a coordinate value of "value" is greater than one of the "max" coordinate, then this "value" coordinate is set with the "max" one
     * @param value - defines the current value
     * @param min - defines the lower range value
     * @param max - defines the upper range value
     * @param result - defines the Vector3 where to store the result
     */
    function clampToRef(value, min, max, result) {
        let x = value.x;
        x = x > max.x ? max.x : x;
        x = x < min.x ? min.x : x;
        let y = value.y;
        y = y > max.y ? max.y : y;
        y = y < min.y ? min.y : y;
        let z = value.z;
        z = z > max.z ? max.z : z;
        z = z < min.z ? min.z : z;
        copyFromFloats(x, y, z, result);
    }
    Vector3.clampToRef = clampToRef;
    /**
     * Returns a new Vector3 located for "amount" (float) on the Hermite interpolation spline defined by the vectors "value1", "tangent1", "value2", "tangent2"
     * @param value1 - defines the first control point
     * @param tangent1 - defines the first tangent vector
     * @param value2 - defines the second control point
     * @param tangent2 - defines the second tangent vector
     * @param amount - defines the amount on the interpolation spline (between 0 and 1)
     * @returns the new Vector3
     */
    function hermite(value1, tangent1, value2, tangent2, amount) {
        const squared = amount * amount;
        const cubed = amount * squared;
        const part1 = 2.0 * cubed - 3.0 * squared + 1.0;
        const part2 = -2.0 * cubed + 3.0 * squared;
        const part3 = cubed - 2.0 * squared + amount;
        const part4 = cubed - squared;
        const x = value1.x * part1 +
            value2.x * part2 +
            tangent1.x * part3 +
            tangent2.x * part4;
        const y = value1.y * part1 +
            value2.y * part2 +
            tangent1.y * part3 +
            tangent2.y * part4;
        const z = value1.z * part1 +
            value2.z * part2 +
            tangent1.z * part3 +
            tangent2.z * part4;
        return create(x, y, z);
    }
    Vector3.hermite = hermite;
    /**
     * Gets the minimal coordinate values between two Vector3
     * @param left - defines the first operand
     * @param right - defines the second operand
     * @returns the new Vector3
     */
    function minimize(left, right) {
        const min = create();
        minimizeInPlaceFromFloatsToRef(right, left.x, left.y, left.z, min);
        return min;
    }
    Vector3.minimize = minimize;
    /**
     * Gets the maximal coordinate values between two Vector3
     * @param left - defines the first operand
     * @param right - defines the second operand
     * @returns the new Vector3
     */
    function maximize(left, right) {
        const max = create();
        maximizeInPlaceFromFloatsToRef(left, right.x, right.y, right.z, max);
        return max;
    }
    Vector3.maximize = maximize;
    /**
     * Returns the distance between the vectors "value1" and "value2"
     * @param value1 - defines the first operand
     * @param value2 - defines the second operand
     * @returns the distance
     */
    function distance(value1, value2) {
        return Math.sqrt(distanceSquared(value1, value2));
    }
    Vector3.distance = distance;
    /**
     * Returns the squared distance between the vectors "value1" and "value2"
     * @param value1 - defines the first operand
     * @param value2 - defines the second operand
     * @returns the squared distance
     */
    function distanceSquared(value1, value2) {
        const x = value1.x - value2.x;
        const y = value1.y - value2.y;
        const z = value1.z - value2.z;
        return x * x + y * y + z * z;
    }
    Vector3.distanceSquared = distanceSquared;
    /**
     * Returns a new Vector3 located at the center between "value1" and "value2"
     * @param value1 - defines the first operand
     * @param value2 - defines the second operand
     * @returns the new Vector3
     */
    function center(value1, value2) {
        const center = add(value1, value2);
        scaleToRef(center, 0.5, center);
        return center;
    }
    Vector3.center = center;
    /**
     * Given three orthogonal normalized left-handed oriented Vector3 axis in space (target system),
     * RotationFromAxis() returns the rotation Euler angles (ex : rotation.x, rotation.y, rotation.z) to apply
     * to something in order to rotate it from its local system to the given target system
     * Note: axis1, axis2 and axis3 are normalized during this operation
     * @param axis1 - defines the first axis
     * @param axis2 - defines the second axis
     * @param axis3 - defines the third axis
     * @returns a new Vector3
     */
    function rotationFromAxis(axis1, axis2, axis3) {
        const rotation = Zero();
        rotationFromAxisToRef(axis1, axis2, axis3, rotation);
        return rotation;
    }
    Vector3.rotationFromAxis = rotationFromAxis;
    /**
     * The same than RotationFromAxis but updates the given ref Vector3 parameter instead of returning a new Vector3
     * @param axis1 - defines the first axis
     * @param axis2 - defines the second axis
     * @param axis3 - defines the third axis
     * @param ref - defines the Vector3 where to store the result
     */
    function rotationFromAxisToRef(axis1, axis2, axis3, result) {
        const quat = Quaternion.create();
        Quaternion.fromAxisToRotationQuaternionToRef(axis1, axis2, axis3, quat);
        copyFrom(Quaternion.toEulerAngles(quat), result);
    }
    Vector3.rotationFromAxisToRef = rotationFromAxisToRef;
    /**
     * Creates a string representation of the Vector3
     * @returns a string with the Vector3 coordinates.
     */
    function toString(vector) {
        return `(${vector.x}, ${vector.y}, ${vector.z})`;
    }
    Vector3.toString = toString;
    /**
     * Creates the Vector3 hash code
     * @returns a number which tends to be unique between Vector3 instances
     */
    function getHashCode(vector) {
        let hash = vector.x || 0;
        hash = (hash * 397) ^ (vector.y || 0);
        hash = (hash * 397) ^ (vector.z || 0);
        return hash;
    }
    Vector3.getHashCode = getHashCode;
    /**
     * Returns true if the vector1 and the vector2 coordinates are strictly equal
     * @param vector1 - defines the first operand
     * @param vector2 - defines the second operand
     * @returns true if both vectors are equals
     */
    function equals(vector1, vector2) {
        return (vector1.x === vector2.x &&
            vector1.y === vector2.y &&
            vector1.z === vector2.z);
    }
    Vector3.equals = equals;
    /**
     * Returns true if the current Vector3 and the given vector coordinates are distant less than epsilon
     * @param otherVector - defines the second operand
     * @param epsilon - defines the minimal distance to define values as equals
     * @returns true if both vectors are distant less than epsilon
     */
    function equalsWithEpsilon(vector1, vector2, epsilon = Epsilon) {
        return (Scalar.withinEpsilon(vector1.x, vector2.x, epsilon) &&
            Scalar.withinEpsilon(vector1.y, vector2.y, epsilon) &&
            Scalar.withinEpsilon(vector1.z, vector2.z, epsilon));
    }
    Vector3.equalsWithEpsilon = equalsWithEpsilon;
    /**
     * Returns true if the current Vector3 coordinates equals the given floats
     * @param x - defines the x coordinate of the operand
     * @param y - defines the y coordinate of the operand
     * @param z - defines the z coordinate of the operand
     * @returns true if both vectors are equals
     */
    function equalsToFloats(vector, x, y, z) {
        return vector.x === x && vector.y === y && vector.z === z;
    }
    Vector3.equalsToFloats = equalsToFloats;
    /**
     * Returns a new Vector3, result of the multiplication of vector1 by the vector2
     * @param vector1 - defines the first operand
     * @param vector2 - defines the second operand
     * @returns the new Vector3
     */
    function multiply(vector1, vector2) {
        const result = create();
        multiplyToRef(vector1, vector2, result);
        return result;
    }
    Vector3.multiply = multiply;
    /**
     * Multiplies the current Vector3 by the given one and stores the result in the given vector "result"
     * @param otherVector - defines the second operand
     * @param result - defines the Vector3 object where to store the result
     * @returns the current Vector3
     */
    function multiplyToRef(vector1, vector2, result) {
        result.x = vector1.x * vector2.x;
        result.y = vector1.y * vector2.y;
        result.z = vector1.z * vector2.z;
    }
    Vector3.multiplyToRef = multiplyToRef;
    /**
     * Returns a new Vector3 set with the result of the mulliplication of the current Vector3 coordinates by the given floats
     * @param x - defines the x coordinate of the operand
     * @param y - defines the y coordinate of the operand
     * @param z - defines the z coordinate of the operand
     * @returns the new Vector3
     */
    function multiplyByFloatsToRef(vector1, x, y, z, result) {
        result.x = vector1.x * x;
        result.y = vector1.y * y;
        result.z = vector1.z * z;
    }
    Vector3.multiplyByFloatsToRef = multiplyByFloatsToRef;
    /**
     * Returns a new Vector3 set with the result of the mulliplication of the current Vector3 coordinates by the given floats
     * @param x - defines the x coordinate of the operand
     * @param y - defines the y coordinate of the operand
     * @param z - defines the z coordinate of the operand
     * @returns the new Vector3
     */
    function multiplyByFloats(vector1, x, y, z) {
        const result = create();
        multiplyByFloatsToRef(vector1, x, y, z, result);
        return result;
    }
    Vector3.multiplyByFloats = multiplyByFloats;
    /**
     * Returns a new Vector3 set with the result of the division of the current Vector3 coordinates by the given ones
     * @param otherVector - defines the second operand
     * @returns the new Vector3
     */
    function divide(vector1, vector2) {
        return {
            x: vector1.x / vector2.x,
            y: vector1.y / vector2.y,
            z: vector1.z / vector2.z
        };
    }
    Vector3.divide = divide;
    /**
     * Divides the current Vector3 coordinates by the given ones and stores the result in the given vector "result"
     * @param otherVector - defines the second operand
     * @param result - defines the Vector3 object where to store the result
     * @returns the current Vector3
     */
    function divideToRef(vector1, vector2, result) {
        result.x = vector1.x / vector2.x;
        result.y = vector1.y / vector2.y;
        result.z = vector1.z / vector2.z;
    }
    Vector3.divideToRef = divideToRef;
    /**
     * Set result Vector3 with the maximal coordinate values between vector1 and the given coordinates
     * @param x - defines the x coordinate of the operand
     * @param y - defines the y coordinate of the operand
     * @param z - defines the z coordinate of the operand
     * @param result - the set Vector3
     */
    function maximizeInPlaceFromFloatsToRef(vector1, x, y, z, result) {
        if (x > vector1.x) {
            result.x = x;
        }
        else {
            result.x = vector1.x;
        }
        if (y > vector1.y) {
            result.y = y;
        }
        else {
            result.y = vector1.y;
        }
        if (z > vector1.z) {
            result.z = z;
        }
        else {
            result.z = vector1.z;
        }
    }
    Vector3.maximizeInPlaceFromFloatsToRef = maximizeInPlaceFromFloatsToRef;
    /**
     * Set result Vector3 with the minimal coordinate values between vector1 and the given coordinates
     * @param x - defines the x coordinate of the operand
     * @param y - defines the y coordinate of the operand
     * @param z - defines the z coordinate of the operand
     * @param result - the set Vector3
     */
    function minimizeInPlaceFromFloatsToRef(vector1, x, y, z, result) {
        if (x < vector1.x) {
            result.x = x;
        }
        else {
            result.x = vector1.x;
        }
        if (y < vector1.y) {
            result.y = y;
        }
        else {
            result.y = vector1.y;
        }
        if (z < vector1.z) {
            result.z = z;
        }
        else {
            result.z = vector1.z;
        }
    }
    Vector3.minimizeInPlaceFromFloatsToRef = minimizeInPlaceFromFloatsToRef;
    /**
     * Gets a new Vector3 from vector1 floored values
     * @returns a new Vector3
     */
    function floor(vector1) {
        return create(Math.floor(vector1.x), Math.floor(vector1.y), Math.floor(vector1.z));
    }
    Vector3.floor = floor;
    /**
     * Gets a new Vector3 from vector1 floored values
     * @returns a new Vector3
     */
    function fract(vector1) {
        return create(vector1.x - Math.floor(vector1.x), vector1.y - Math.floor(vector1.y), vector1.z - Math.floor(vector1.z));
    }
    Vector3.fract = fract;
    /**
     * Returns a new Vector3 set to (0.0, 0.0, 0.0)
     * @returns a new empty Vector3
     */
    function Zero() {
        return create(0.0, 0.0, 0.0);
    }
    Vector3.Zero = Zero;
    /**
     * Returns a new Vector3 set to (1.0, 1.0, 1.0)
     * @returns a new unit Vector3
     */
    function One() {
        return create(1.0, 1.0, 1.0);
    }
    Vector3.One = One;
    /**
     * Returns a new Vector3 set tolengthSquared (0.0, 1.0, 0.0)
     * @returns a new up Vector3
     */
    function Up() {
        return create(0.0, 1.0, 0.0);
    }
    Vector3.Up = Up;
    /**
     * Returns a new Vector3 set to (0.0, -1.0, 0.0)
     * @returns a new down Vector3
     */
    function Down() {
        return create(0.0, -1.0, 0.0);
    }
    Vector3.Down = Down;
    /**
     * Returns a new Vector3 set to (0.0, 0.0, 1.0)
     * @returns a new forward Vector3
     */
    function Forward() {
        return create(0.0, 0.0, 1.0);
    }
    Vector3.Forward = Forward;
    /**
     * Returns a new Vector3 set to (0.0, 0.0, -1.0)
     * @returns a new forward Vector3
     */
    function Backward() {
        return create(0.0, 0.0, -1.0);
    }
    Vector3.Backward = Backward;
    /**
     * Returns a new Vector3 set to (1.0, 0.0, 0.0)
     * @returns a new right Vector3
     */
    function Right() {
        return create(1.0, 0.0, 0.0);
    }
    Vector3.Right = Right;
    /**
     * Returns a new Vector3 set to (-1.0, 0.0, 0.0)
     * @returns a new left Vector3
     */
    function Left() {
        return create(-1.0, 0.0, 0.0);
    }
    Vector3.Left = Left;
    /**
     * Returns a new random Vector3
     * @returns a random Vector3
     */
    function Random() {
        return create(Math.random(), Math.random(), Math.random());
    }
    Vector3.Random = Random;
})(Vector3 || (Vector3 = {}));

/**
 * Represens a plane by the equation ax + by + cz + d = 0
 * @public
 */
var Plane;
(function (Plane) {
    /**
     * Creates a Plane object according to the given floats a, b, c, d and the plane equation : ax + by + cz + d = 0
     * @param a - a component of the plane
     * @param b - b component of the plane
     * @param c - c component of the plane
     * @param d - d component of the plane
     */
    function create(a, b, c, d) {
        return {
            normal: Vector3.create(a, b, c),
            d: d
        };
    }
    Plane.create = create;
    // Statics
    /**
     * Creates a plane from an  array
     * @param array - the array to create a plane from
     * @returns a new Plane from the given array.
     */
    function fromArray(array) {
        return create(array[0], array[1], array[2], array[3]);
    }
    Plane.fromArray = fromArray;
    /**
     * Creates a plane from three points
     * @param point1 - point used to create the plane
     * @param point2 - point used to create the plane
     * @param point3 - point used to create the plane
     * @returns a new Plane defined by the three given points.
     */
    function fromPoints(_point1, _point2, _point3) {
        const result = create(0.0, 0.0, 0.0, 0.0);
        // TODO
        // result.copyFromPoints(point1, point2, point3)
        return result;
    }
    Plane.fromPoints = fromPoints;
    /**
     * Creates a plane from an origin point and a normal
     * @param origin - origin of the plane to be constructed
     * @param normal - normal of the plane to be constructed
     * @returns a new Plane the normal vector to this plane at the given origin point.
     * Note : the vector "normal" is updated because normalized.
     */
    function romPositionAndNormal(origin, normal) {
        const result = create(0.0, 0.0, 0.0, 0.0);
        result.normal = Vector3.normalize(normal);
        result.d = -(normal.x * origin.x +
            normal.y * origin.y +
            normal.z * origin.z);
        return result;
    }
    Plane.romPositionAndNormal = romPositionAndNormal;
    /**
     * Calculates the distance from a plane and a point
     * @param origin - origin of the plane to be constructed
     * @param normal - normal of the plane to be constructed
     * @param point - point to calculate distance to
     * @returns the signed distance between the plane defined by the normal vector at the "origin"" point and the given other point.
     */
    function signedDistanceToPlaneFromPositionAndNormal(origin, normal, point) {
        const d = -(normal.x * origin.x + normal.y * origin.y + normal.z * origin.z);
        return Vector3.dot(point, normal) + d;
    }
    Plane.signedDistanceToPlaneFromPositionAndNormal = signedDistanceToPlaneFromPositionAndNormal;
    /**
     * @returns the plane coordinates as a new array of 4 elements [a, b, c, d].
     */
    function asArray(plane) {
        return [plane.normal.x, plane.normal.y, plane.normal.z, plane.d];
    }
    Plane.asArray = asArray;
    // Methods
    /**
     * @returns a new plane copied from the current Plane.
     */
    function clone(plane) {
        return create(plane.normal.x, plane.normal.y, plane.normal.z, plane.d);
    }
    Plane.clone = clone;
    /**
     * @returns the Plane hash code.
     */
    function getHashCode(_plane) {
        // TODO
        // let hash = plane.normal.getHashCode()
        // hash = (hash * 397) ^ (plane.d || 0)
        // return hash
        return 0;
    }
    Plane.getHashCode = getHashCode;
    /**
     * Normalize the current Plane in place.
     * @returns the updated Plane.
     */
    function normalize(plane) {
        const result = create(0, 0, 0, 0);
        const norm = Math.sqrt(plane.normal.x * plane.normal.x +
            plane.normal.y * plane.normal.y +
            plane.normal.z * plane.normal.z);
        let magnitude = 0.0;
        if (norm !== 0) {
            magnitude = 1.0 / norm;
        }
        result.normal.x = plane.normal.x * magnitude;
        result.normal.y = plane.normal.y * magnitude;
        result.normal.z = plane.normal.z * magnitude;
        result.d *= magnitude;
        return plane;
    }
    Plane.normalize = normalize;
    /**
     * Applies a transformation the plane and returns the result
     * @param transformation - the transformation matrix to be applied to the plane
     * @returns a new Plane as the result of the transformation of the current Plane by the given matrix.
     */
    function transform(plane, transformation) {
        const transposedMatrix = Matrix.create();
        Matrix.transposeToRef(transformation, transposedMatrix);
        const m = transposedMatrix._m;
        const x = plane.normal.x;
        const y = plane.normal.y;
        const z = plane.normal.z;
        const d = plane.d;
        const normalX = x * m[0] + y * m[1] + z * m[2] + d * m[3];
        const normalY = x * m[4] + y * m[5] + z * m[6] + d * m[7];
        const normalZ = x * m[8] + y * m[9] + z * m[10] + d * m[11];
        const finalD = x * m[12] + y * m[13] + z * m[14] + d * m[15];
        return create(normalX, normalY, normalZ, finalD);
    }
    Plane.transform = transform;
    /**
     * Calcualtte the dot product between the point and the plane normal
     * @param point - point to calculate the dot product with
     * @returns the dot product (float) of the point coordinates and the plane normal.
     */
    function dotCoordinate(plane, point) {
        return (plane.normal.x * point.x +
            plane.normal.y * point.y +
            plane.normal.z * point.z +
            plane.d);
    }
    Plane.dotCoordinate = dotCoordinate;
    /**
     * Updates the current Plane from the plane defined by the three given points.
     * @param point1 - one of the points used to contruct the plane
     * @param point2 - one of the points used to contruct the plane
     * @param point3 - one of the points used to contruct the plane
     * @returns the updated Plane.
     */
    function copyFromPoints(point1, point2, point3) {
        const x1 = point2.x - point1.x;
        const y1 = point2.y - point1.y;
        const z1 = point2.z - point1.z;
        const x2 = point3.x - point1.x;
        const y2 = point3.y - point1.y;
        const z2 = point3.z - point1.z;
        const yz = y1 * z2 - z1 * y2;
        const xz = z1 * x2 - x1 * z2;
        const xy = x1 * y2 - y1 * x2;
        const pyth = Math.sqrt(yz * yz + xz * xz + xy * xy);
        let invPyth;
        if (pyth !== 0) {
            invPyth = 1.0 / pyth;
        }
        else {
            invPyth = 0.0;
        }
        const normal = Vector3.create(yz * invPyth, xz * invPyth, xy * invPyth);
        return {
            normal,
            d: -(normal.x * point1.x + normal.y * point1.y + normal.z * point1.z)
        };
    }
    Plane.copyFromPoints = copyFromPoints;
    /**
     * Checks if the plane is facing a given direction
     * @param direction - the direction to check if the plane is facing
     * @param epsilon - value the dot product is compared against (returns true if dot &lt;= epsilon)
     * @returns True is the vector "direction"  is the same side than the plane normal.
     */
    function isFrontFacingTo(plane, direction, epsilon) {
        const dot = Vector3.dot(plane.normal, direction);
        return dot <= epsilon;
    }
    Plane.isFrontFacingTo = isFrontFacingTo;
    /**
     * Calculates the distance to a point
     * @param point - point to calculate distance to
     * @returns the signed distance (float) from the given point to the Plane.
     */
    function signedDistanceTo(plane, point) {
        return Vector3.dot(point, plane.normal) + plane.d;
    }
    Plane.signedDistanceTo = signedDistanceTo;
})(Plane || (Plane = {}));

/**
 * Class used to store matrix data (4x4)
 * @public
 */
var Matrix;
(function (Matrix) {
    /**
     * Gets the internal data of the matrix
     */
    function m(self) {
        return self._m;
    }
    Matrix.m = m;
    let _updateFlagSeed = 0;
    const _identityReadonly = {};
    /**
     * Gets an identity matrix that must not be updated
     */
    function IdentityReadonly() {
        return _identityReadonly;
    }
    Matrix.IdentityReadonly = IdentityReadonly;
    /**
     * Creates an empty matrix (filled with zeros)
     */
    function create() {
        const newMatrix = {
            updateFlag: 0,
            isIdentity: false,
            isIdentity3x2: true,
            _isIdentityDirty: true,
            _isIdentity3x2Dirty: true,
            _m: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
        };
        _updateIdentityStatus(newMatrix, false);
        return newMatrix;
    }
    Matrix.create = create;
    // Statics
    /**
     * Creates a matrix from an array
     * @param array - defines the source array
     * @param offset - defines an offset in the source array
     * @returns a new Matrix set from the starting index of the given array
     */
    function fromArray(array, offset = 0) {
        const result = create();
        fromArrayToRef(array, offset, result);
        return result;
    }
    Matrix.fromArray = fromArray;
    /**
     * Copy the content of an array into a given matrix
     * @param array - defines the source array
     * @param offset - defines an offset in the source array
     * @param result - defines the target matrix
     */
    function fromArrayToRef(array, offset, result) {
        for (let index = 0; index < 16; index++) {
            result._m[index] = array[index + offset];
        }
        _markAsUpdated(result);
    }
    Matrix.fromArrayToRef = fromArrayToRef;
    /**
     * Stores an array into a matrix after having multiplied each component by a given factor
     * @param array - defines the source array
     * @param offset - defines the offset in the source array
     * @param scale - defines the scaling factor
     * @param result - defines the target matrix
     */
    function fromFloatArrayToRefScaled(array, offset, scale, result) {
        for (let index = 0; index < 16; index++) {
            result._m[index] = array[index + offset] * scale;
        }
        _markAsUpdated(result);
    }
    Matrix.fromFloatArrayToRefScaled = fromFloatArrayToRefScaled;
    /**
     * Stores a list of values (16) inside a given matrix
     * @param initialM11 - defines 1st value of 1st row
     * @param initialM12 - defines 2nd value of 1st row
     * @param initialM13 - defines 3rd value of 1st row
     * @param initialM14 - defines 4th value of 1st row
     * @param initialM21 - defines 1st value of 2nd row
     * @param initialM22 - defines 2nd value of 2nd row
     * @param initialM23 - defines 3rd value of 2nd row
     * @param initialM24 - defines 4th value of 2nd row
     * @param initialM31 - defines 1st value of 3rd row
     * @param initialM32 - defines 2nd value of 3rd row
     * @param initialM33 - defines 3rd value of 3rd row
     * @param initialM34 - defines 4th value of 3rd row
     * @param initialM41 - defines 1st value of 4th row
     * @param initialM42 - defines 2nd value of 4th row
     * @param initialM43 - defines 3rd value of 4th row
     * @param initialM44 - defines 4th value of 4th row
     * @param result - defines the target matrix
     */
    function fromValuesToRef(initialM11, initialM12, initialM13, initialM14, initialM21, initialM22, initialM23, initialM24, initialM31, initialM32, initialM33, initialM34, initialM41, initialM42, initialM43, initialM44, result) {
        const m = result._m;
        m[0] = initialM11;
        m[1] = initialM12;
        m[2] = initialM13;
        m[3] = initialM14;
        m[4] = initialM21;
        m[5] = initialM22;
        m[6] = initialM23;
        m[7] = initialM24;
        m[8] = initialM31;
        m[9] = initialM32;
        m[10] = initialM33;
        m[11] = initialM34;
        m[12] = initialM41;
        m[13] = initialM42;
        m[14] = initialM43;
        m[15] = initialM44;
        _markAsUpdated(result);
    }
    Matrix.fromValuesToRef = fromValuesToRef;
    /**
     * Creates new matrix from a list of values (16)
     * @param initialM11 - defines 1st value of 1st row
     * @param initialM12 - defines 2nd value of 1st row
     * @param initialM13 - defines 3rd value of 1st row
     * @param initialM14 - defines 4th value of 1st row
     * @param initialM21 - defines 1st value of 2nd row
     * @param initialM22 - defines 2nd value of 2nd row
     * @param initialM23 - defines 3rd value of 2nd row
     * @param initialM24 - defines 4th value of 2nd row
     * @param initialM31 - defines 1st value of 3rd row
     * @param initialM32 - defines 2nd value of 3rd row
     * @param initialM33 - defines 3rd value of 3rd row
     * @param initialM34 - defines 4th value of 3rd row
     * @param initialM41 - defines 1st value of 4th row
     * @param initialM42 - defines 2nd value of 4th row
     * @param initialM43 - defines 3rd value of 4th row
     * @param initialM44 - defines 4th value of 4th row
     * @returns the new matrix
     */
    function fromValues(initialM11, initialM12, initialM13, initialM14, initialM21, initialM22, initialM23, initialM24, initialM31, initialM32, initialM33, initialM34, initialM41, initialM42, initialM43, initialM44) {
        const result = create();
        const m = result._m;
        m[0] = initialM11;
        m[1] = initialM12;
        m[2] = initialM13;
        m[3] = initialM14;
        m[4] = initialM21;
        m[5] = initialM22;
        m[6] = initialM23;
        m[7] = initialM24;
        m[8] = initialM31;
        m[9] = initialM32;
        m[10] = initialM33;
        m[11] = initialM34;
        m[12] = initialM41;
        m[13] = initialM42;
        m[14] = initialM43;
        m[15] = initialM44;
        _markAsUpdated(result);
        return result;
    }
    Matrix.fromValues = fromValues;
    /**
     * Creates a new matrix composed by merging scale (vector3), rotation (quaternion) and translation (vector3)
     * @param scale - defines the scale vector3
     * @param rotation - defines the rotation quaternion
     * @param translation - defines the translation vector3
     * @returns a new matrix
     */
    function compose(scale, rotation, translation) {
        const result = create();
        composeToRef(scale, rotation, translation, result);
        return result;
    }
    Matrix.compose = compose;
    /**
     * Sets a matrix to a value composed by merging scale (vector3), rotation (quaternion) and translation (vector3)
     * @param scale - defines the scale vector3
     * @param rotation - defines the rotation quaternion
     * @param translation - defines the translation vector3
     * @param result - defines the target matrix
     */
    function composeToRef(scale, rotation, translation, result) {
        const tmpMatrix = [create(), create(), create()];
        scalingToRef(scale.x, scale.y, scale.z, tmpMatrix[1]);
        fromQuaternionToRef(rotation, tmpMatrix[0]);
        multiplyToRef(tmpMatrix[1], tmpMatrix[0], result);
        setTranslation(result, translation);
    }
    Matrix.composeToRef = composeToRef;
    /**
     * Creates a new identity matrix
     * @returns a new identity matrix
     */
    function Identity() {
        const identity = fromValues(1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0);
        _updateIdentityStatus(identity, true);
        return identity;
    }
    Matrix.Identity = Identity;
    /**
     * Creates a new identity matrix and stores the result in a given matrix
     * @param result - defines the target matrix
     */
    function IdentityToRef(result) {
        fromValuesToRef(1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, result);
        _updateIdentityStatus(result, true);
    }
    Matrix.IdentityToRef = IdentityToRef;
    /**
     * Creates a new zero matrix
     * @returns a new zero matrix
     */
    function Zero() {
        const zero = fromValues(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        _updateIdentityStatus(zero, false);
        return zero;
    }
    Matrix.Zero = Zero;
    /**
     * Creates a new rotation matrix for "angle" radians around the X axis
     * @param angle - defines the angle (in radians) to use
     * @returns the new matrix
     */
    function RotationX(angle) {
        const result = create();
        rotationXToRef(angle, result);
        return result;
    }
    Matrix.RotationX = RotationX;
    /**
     * Creates a new rotation matrix for "angle" radians around the X axis and stores it in a given matrix
     * @param angle - defines the angle (in radians) to use
     * @param result - defines the target matrix
     */
    function rotationXToRef(angle, result) {
        const s = Math.sin(angle);
        const c = Math.cos(angle);
        fromValuesToRef(1.0, 0.0, 0.0, 0.0, 0.0, c, s, 0.0, 0.0, -s, c, 0.0, 0.0, 0.0, 0.0, 1.0, result);
        _updateIdentityStatus(result, c === 1 && s === 0);
    }
    Matrix.rotationXToRef = rotationXToRef;
    /**
     * Creates a new rotation matrix for "angle" radians around the Y axis
     * @param angle - defines the angle (in radians) to use
     * @returns the new matrix
     */
    function rotationY(angle) {
        const result = create();
        rotationYToRef(angle, result);
        return result;
    }
    Matrix.rotationY = rotationY;
    /**
     * Creates a new rotation matrix for "angle" radians around the Y axis and stores it in a given matrix
     * @param angle - defines the angle (in radians) to use
     * @param result - defines the target matrix
     */
    function rotationYToRef(angle, result) {
        const s = Math.sin(angle);
        const c = Math.cos(angle);
        fromValuesToRef(c, 0.0, -s, 0.0, 0.0, 1.0, 0.0, 0.0, s, 0.0, c, 0.0, 0.0, 0.0, 0.0, 1.0, result);
        _updateIdentityStatus(result, c === 1 && s === 0);
    }
    Matrix.rotationYToRef = rotationYToRef;
    /**
     * Creates a new rotation matrix for "angle" radians around the Z axis
     * @param angle - defines the angle (in radians) to use
     * @returns the new matrix
     */
    function rotationZ(angle) {
        const result = create();
        rotationZToRef(angle, result);
        return result;
    }
    Matrix.rotationZ = rotationZ;
    /**
     * Creates a new rotation matrix for "angle" radians around the Z axis and stores it in a given matrix
     * @param angle - defines the angle (in radians) to use
     * @param result - defines the target matrix
     */
    function rotationZToRef(angle, result) {
        const s = Math.sin(angle);
        const c = Math.cos(angle);
        fromValuesToRef(c, s, 0.0, 0.0, -s, c, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, result);
        _updateIdentityStatus(result, c === 1 && s === 0);
    }
    Matrix.rotationZToRef = rotationZToRef;
    /**
     * Creates a new rotation matrix for "angle" radians around the given axis
     * @param axis - defines the axis to use
     * @param angle - defines the angle (in radians) to use
     * @returns the new matrix
     */
    function rotationAxis(axis, angle) {
        const result = create();
        rotationAxisToRef(axis, angle, result);
        return result;
    }
    Matrix.rotationAxis = rotationAxis;
    /**
     * Creates a new rotation matrix for "angle" radians around the given axis and stores it in a given matrix
     * @param axis - defines the axis to use
     * @param angle - defines the angle (in radians) to use
     * @param result - defines the target matrix
     */
    function rotationAxisToRef(_axis, angle, result) {
        const s = Math.sin(-angle);
        const c = Math.cos(-angle);
        const c1 = 1 - c;
        const axis = Vector3.normalize(_axis);
        const m = result._m;
        m[0] = axis.x * axis.x * c1 + c;
        m[1] = axis.x * axis.y * c1 - axis.z * s;
        m[2] = axis.x * axis.z * c1 + axis.y * s;
        m[3] = 0.0;
        m[4] = axis.y * axis.x * c1 + axis.z * s;
        m[5] = axis.y * axis.y * c1 + c;
        m[6] = axis.y * axis.z * c1 - axis.x * s;
        m[7] = 0.0;
        m[8] = axis.z * axis.x * c1 - axis.y * s;
        m[9] = axis.z * axis.y * c1 + axis.x * s;
        m[10] = axis.z * axis.z * c1 + c;
        m[11] = 0.0;
        m[12] = 0.0;
        m[13] = 0.0;
        m[14] = 0.0;
        m[15] = 1.0;
        _markAsUpdated(result);
    }
    Matrix.rotationAxisToRef = rotationAxisToRef;
    /**
     * Creates a rotation matrix
     * @param yaw - defines the yaw angle in radians (Y axis)
     * @param pitch - defines the pitch angle in radians (X axis)
     * @param roll - defines the roll angle in radians (X axis)
     * @returns the new rotation matrix
     */
    function rotationYawPitchRoll(yaw, pitch, roll) {
        const result = create();
        rotationYawPitchRollToRef(yaw, pitch, roll, result);
        return result;
    }
    Matrix.rotationYawPitchRoll = rotationYawPitchRoll;
    /**
     * Creates a rotation matrix and stores it in a given matrix
     * @param yaw - defines the yaw angle in radians (Y axis)
     * @param pitch - defines the pitch angle in radians (X axis)
     * @param roll - defines the roll angle in radians (X axis)
     * @param result - defines the target matrix
     */
    function rotationYawPitchRollToRef(yaw, pitch, roll, result) {
        const quaternionResult = Quaternion.Zero();
        Quaternion.fromRotationYawPitchRollToRef(yaw, pitch, roll, quaternionResult);
        fromQuaternionToRef(quaternionResult, result);
    }
    Matrix.rotationYawPitchRollToRef = rotationYawPitchRollToRef;
    /**
     * Creates a scaling matrix
     * @param x - defines the scale factor on X axis
     * @param y - defines the scale factor on Y axis
     * @param z - defines the scale factor on Z axis
     * @returns the new matrix
     */
    function scaling(x, y, z) {
        const result = create();
        scalingToRef(x, y, z, result);
        return result;
    }
    Matrix.scaling = scaling;
    /**
     * Creates a scaling matrix and stores it in a given matrix
     * @param x - defines the scale factor on X axis
     * @param y - defines the scale factor on Y axis
     * @param z - defines the scale factor on Z axis
     * @param result - defines the target matrix
     */
    function scalingToRef(x, y, z, result) {
        fromValuesToRef(x, 0.0, 0.0, 0.0, 0.0, y, 0.0, 0.0, 0.0, 0.0, z, 0.0, 0.0, 0.0, 0.0, 1.0, result);
        _updateIdentityStatus(result, x === 1 && y === 1 && z === 1);
    }
    Matrix.scalingToRef = scalingToRef;
    /**
     * Creates a translation matrix
     * @param x - defines the translation on X axis
     * @param y - defines the translation on Y axis
     * @param z - defines the translationon Z axis
     * @returns the new matrix
     */
    function translation(x, y, z) {
        const result = create();
        translationToRef(x, y, z, result);
        return result;
    }
    Matrix.translation = translation;
    /**
     * Creates a translation matrix and stores it in a given matrix
     * @param x - defines the translation on X axis
     * @param y - defines the translation on Y axis
     * @param z - defines the translationon Z axis
     * @param result - defines the target matrix
     */
    function translationToRef(x, y, z, result) {
        fromValuesToRef(1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, x, y, z, 1.0, result);
        _updateIdentityStatus(result, x === 0 && y === 0 && z === 0);
    }
    Matrix.translationToRef = translationToRef;
    /**
     * Returns a new Matrix whose values are the interpolated values for "gradient" (float) between the ones of the matrices "startValue" and "endValue".
     * @param startValue - defines the start value
     * @param endValue - defines the end value
     * @param gradient - defines the gradient factor
     * @returns the new matrix
     */
    function lerp(startValue, endValue, gradient) {
        const result = create();
        lerpToRef(startValue, endValue, gradient, result);
        return result;
    }
    Matrix.lerp = lerp;
    /**
     * Set the given matrix "result" as the interpolated values for "gradient" (float) between the ones of the matrices "startValue" and "endValue".
     * @param startValue - defines the start value
     * @param endValue - defines the end value
     * @param gradient - defines the gradient factor
     * @param result - defines the Matrix object where to store data
     */
    function lerpToRef(startValue, endValue, gradient, result) {
        for (let index = 0; index < 16; index++) {
            result._m[index] =
                startValue._m[index] * (1.0 - gradient) + endValue._m[index] * gradient;
        }
        _markAsUpdated(result);
    }
    Matrix.lerpToRef = lerpToRef;
    /**
     * Builds a new matrix whose values are computed by:
     * * decomposing the the "startValue" and "endValue" matrices into their respective scale, rotation and translation matrices
     * * interpolating for "gradient" (float) the values between each of these decomposed matrices between the start and the end
     * * recomposing a new matrix from these 3 interpolated scale, rotation and translation matrices
     * @param startValue - defines the first matrix
     * @param endValue - defines the second matrix
     * @param gradient - defines the gradient between the two matrices
     * @returns the new matrix
     */
    function decomposeLerp(startValue, endValue, gradient) {
        const result = create();
        decomposeLerpToRef(startValue, endValue, gradient, result);
        return result;
    }
    Matrix.decomposeLerp = decomposeLerp;
    /**
     * Update a matrix to values which are computed by:
     * * decomposing the the "startValue" and "endValue" matrices into their respective scale, rotation and translation matrices
     * * interpolating for "gradient" (float) the values between each of these decomposed matrices between the start and the end
     * * recomposing a new matrix from these 3 interpolated scale, rotation and translation matrices
     * @param startValue - defines the first matrix
     * @param endValue - defines the second matrix
     * @param gradient - defines the gradient between the two matrices
     * @param result - defines the target matrix
     */
    function decomposeLerpToRef(startValue, endValue, gradient, result) {
        const startScale = Vector3.Zero();
        const startRotation = Quaternion.Zero();
        const startTranslation = Vector3.Zero();
        decompose(startValue, startScale, startRotation, startTranslation);
        const endScale = Vector3.Zero();
        const endRotation = Quaternion.Zero();
        const endTranslation = Vector3.Zero();
        decompose(endValue, endScale, endRotation, endTranslation);
        const resultScale = Vector3.Zero();
        Vector3.lerpToRef(startScale, endScale, gradient, resultScale);
        const resultRotation = Quaternion.Zero();
        Quaternion.slerpToRef(startRotation, endRotation, gradient, resultRotation);
        const resultTranslation = Vector3.Zero();
        Vector3.lerpToRef(startTranslation, endTranslation, gradient, resultTranslation);
        composeToRef(resultScale, resultRotation, resultTranslation, result);
    }
    Matrix.decomposeLerpToRef = decomposeLerpToRef;
    /**
     * Gets a new rotation matrix used to rotate an entity so as it looks at the target vector3, from the eye vector3 position, the up vector3 being oriented like "up"
     * self function works in left handed mode
     * @param eye - defines the final position of the entity
     * @param target - defines where the entity should look at
     * @param up - defines the up vector for the entity
     * @returns the new matrix
     */
    function LookAtLH(eye, target, up) {
        const result = create();
        lookAtLHToRef(eye, target, up, result);
        return result;
    }
    Matrix.LookAtLH = LookAtLH;
    /**
     * Sets the given "result" Matrix to a rotation matrix used to rotate an entity so that it looks at the target vector3, from the eye vector3 position, the up vector3 being oriented like "up".
     * self function works in left handed mode
     * @param eye - defines the final position of the entity
     * @param target - defines where the entity should look at
     * @param up - defines the up vector for the entity
     * @param result - defines the target matrix
     */
    function lookAtLHToRef(eye, target, up, result) {
        const xAxis = Vector3.Zero();
        const yAxis = Vector3.Zero();
        const zAxis = Vector3.Zero();
        // Z axis
        Vector3.subtractToRef(target, eye, zAxis);
        Vector3.normalizeToRef(zAxis, zAxis);
        // X axis
        Vector3.crossToRef(up, zAxis, xAxis);
        const xSquareLength = Vector3.lengthSquared(xAxis);
        if (xSquareLength === 0) {
            xAxis.x = 1.0;
        }
        else {
            Vector3.normalizeFromLengthToRef(xAxis, Math.sqrt(xSquareLength), xAxis);
        }
        // Y axis
        Vector3.crossToRef(zAxis, xAxis, yAxis);
        Vector3.normalizeToRef(yAxis, yAxis);
        // Eye angles
        const ex = -Vector3.dot(xAxis, eye);
        const ey = -Vector3.dot(yAxis, eye);
        const ez = -Vector3.dot(zAxis, eye);
        fromValuesToRef(xAxis.x, yAxis.x, zAxis.x, 0.0, xAxis.y, yAxis.y, zAxis.y, 0.0, xAxis.z, yAxis.z, zAxis.z, 0.0, ex, ey, ez, 1.0, result);
    }
    Matrix.lookAtLHToRef = lookAtLHToRef;
    /**
     * Gets a new rotation matrix used to rotate an entity so as it looks at the target vector3, from the eye vector3 position, the up vector3 being oriented like "up"
     * self function works in right handed mode
     * @param eye - defines the final position of the entity
     * @param target - defines where the entity should look at
     * @param up - defines the up vector for the entity
     * @returns the new matrix
     */
    function lookAtRH(eye, target, up) {
        const result = create();
        lookAtRHToRef(eye, target, up, result);
        return result;
    }
    Matrix.lookAtRH = lookAtRH;
    /**
     * Sets the given "result" Matrix to a rotation matrix used to rotate an entity so that it looks at the target vector3, from the eye vector3 position, the up vector3 being oriented like "up".
     * self function works in right handed mode
     * @param eye - defines the final position of the entity
     * @param target - defines where the entity should look at
     * @param up - defines the up vector for the entity
     * @param result - defines the target matrix
     */
    function lookAtRHToRef(eye, target, up, result) {
        const xAxis = Vector3.Zero();
        const yAxis = Vector3.Zero();
        const zAxis = Vector3.Zero();
        // Z axis
        Vector3.subtractToRef(eye, target, zAxis);
        Vector3.normalizeToRef(zAxis, zAxis);
        // X axis
        Vector3.crossToRef(up, zAxis, xAxis);
        const xSquareLength = Vector3.lengthSquared(xAxis);
        if (xSquareLength === 0) {
            xAxis.x = 1.0;
        }
        else {
            Vector3.normalizeFromLengthToRef(xAxis, Math.sqrt(xSquareLength), xAxis);
        }
        // Y axis
        Vector3.crossToRef(zAxis, xAxis, yAxis);
        Vector3.normalizeToRef(yAxis, yAxis);
        // Eye angles
        const ex = -Vector3.dot(xAxis, eye);
        const ey = -Vector3.dot(yAxis, eye);
        const ez = -Vector3.dot(zAxis, eye);
        fromValuesToRef(xAxis.x, yAxis.x, zAxis.x, 0.0, xAxis.y, yAxis.y, zAxis.y, 0.0, xAxis.z, yAxis.z, zAxis.z, 0.0, ex, ey, ez, 1.0, result);
    }
    Matrix.lookAtRHToRef = lookAtRHToRef;
    /**
     * Create a left-handed orthographic projection matrix
     * @param width - defines the viewport width
     * @param height - defines the viewport height
     * @param znear - defines the near clip plane
     * @param zfar - defines the far clip plane
     * @returns a new matrix as a left-handed orthographic projection matrix
     */
    function orthoLH(width, height, znear, zfar) {
        const matrix = create();
        orthoLHToRef(width, height, znear, zfar, matrix);
        return matrix;
    }
    Matrix.orthoLH = orthoLH;
    /**
     * Store a left-handed orthographic projection to a given matrix
     * @param width - defines the viewport width
     * @param height - defines the viewport height
     * @param znear - defines the near clip plane
     * @param zfar - defines the far clip plane
     * @param result - defines the target matrix
     */
    function orthoLHToRef(width, height, znear, zfar, result) {
        const n = znear;
        const f = zfar;
        const a = 2.0 / width;
        const b = 2.0 / height;
        const c = 2.0 / (f - n);
        const d = -(f + n) / (f - n);
        fromValuesToRef(a, 0.0, 0.0, 0.0, 0.0, b, 0.0, 0.0, 0.0, 0.0, c, 0.0, 0.0, 0.0, d, 1.0, result);
        _updateIdentityStatus(result, a === 1 && b === 1 && c === 1 && d === 0);
    }
    Matrix.orthoLHToRef = orthoLHToRef;
    /**
     * Create a left-handed orthographic projection matrix
     * @param left - defines the viewport left coordinate
     * @param right - defines the viewport right coordinate
     * @param bottom - defines the viewport bottom coordinate
     * @param top - defines the viewport top coordinate
     * @param znear - defines the near clip plane
     * @param zfar - defines the far clip plane
     * @returns a new matrix as a left-handed orthographic projection matrix
     */
    function OrthoOffCenterLH(left, right, bottom, top, znear, zfar) {
        const matrix = create();
        orthoOffCenterLHToRef(left, right, bottom, top, znear, zfar, matrix);
        return matrix;
    }
    Matrix.OrthoOffCenterLH = OrthoOffCenterLH;
    /**
     * Stores a left-handed orthographic projection into a given matrix
     * @param left - defines the viewport left coordinate
     * @param right - defines the viewport right coordinate
     * @param bottom - defines the viewport bottom coordinate
     * @param top - defines the viewport top coordinate
     * @param znear - defines the near clip plane
     * @param zfar - defines the far clip plane
     * @param result - defines the target matrix
     */
    function orthoOffCenterLHToRef(left, right, bottom, top, znear, zfar, result) {
        const n = znear;
        const f = zfar;
        const a = 2.0 / (right - left);
        const b = 2.0 / (top - bottom);
        const c = 2.0 / (f - n);
        const d = -(f + n) / (f - n);
        const i0 = (left + right) / (left - right);
        const i1 = (top + bottom) / (bottom - top);
        fromValuesToRef(a, 0.0, 0.0, 0.0, 0.0, b, 0.0, 0.0, 0.0, 0.0, c, 0.0, i0, i1, d, 1.0, result);
        _markAsUpdated(result);
    }
    Matrix.orthoOffCenterLHToRef = orthoOffCenterLHToRef;
    /**
     * Creates a right-handed orthographic projection matrix
     * @param left - defines the viewport left coordinate
     * @param right - defines the viewport right coordinate
     * @param bottom - defines the viewport bottom coordinate
     * @param top - defines the viewport top coordinate
     * @param znear - defines the near clip plane
     * @param zfar - defines the far clip plane
     * @returns a new matrix as a right-handed orthographic projection matrix
     */
    function orthoOffCenterRH(left, right, bottom, top, znear, zfar) {
        const matrix = create();
        orthoOffCenterRHToRef(left, right, bottom, top, znear, zfar, matrix);
        return matrix;
    }
    Matrix.orthoOffCenterRH = orthoOffCenterRH;
    /**
     * Stores a right-handed orthographic projection into a given matrix
     * @param left - defines the viewport left coordinate
     * @param right - defines the viewport right coordinate
     * @param bottom - defines the viewport bottom coordinate
     * @param top - defines the viewport top coordinate
     * @param znear - defines the near clip plane
     * @param zfar - defines the far clip plane
     * @param result - defines the target matrix
     */
    function orthoOffCenterRHToRef(left, right, bottom, top, znear, zfar, result) {
        orthoOffCenterLHToRef(left, right, bottom, top, znear, zfar, result);
        result._m[10] *= -1; // No need to call _markAsUpdated as previous function already called it and let _isIdentityDirty to true
    }
    Matrix.orthoOffCenterRHToRef = orthoOffCenterRHToRef;
    /**
     * Creates a left-handed perspective projection matrix
     * @param width - defines the viewport width
     * @param height - defines the viewport height
     * @param znear - defines the near clip plane
     * @param zfar - defines the far clip plane
     * @returns a new matrix as a left-handed perspective projection matrix
     */
    function perspectiveLH(width, height, znear, zfar) {
        const matrix = create();
        const n = znear;
        const f = zfar;
        const a = (2.0 * n) / width;
        const b = (2.0 * n) / height;
        const c = (f + n) / (f - n);
        const d = (-2.0 * f * n) / (f - n);
        fromValuesToRef(a, 0.0, 0.0, 0.0, 0.0, b, 0.0, 0.0, 0.0, 0.0, c, 1.0, 0.0, 0.0, d, 0.0, matrix);
        _updateIdentityStatus(matrix, false);
        return matrix;
    }
    Matrix.perspectiveLH = perspectiveLH;
    /**
     * Creates a left-handed perspective projection matrix
     * @param fov - defines the horizontal field of view
     * @param aspect - defines the aspect ratio
     * @param znear - defines the near clip plane
     * @param zfar - defines the far clip plane
     * @returns a new matrix as a left-handed perspective projection matrix
     */
    function perspectiveFovLH(fov, aspect, znear, zfar) {
        const matrix = create();
        perspectiveFovLHToRef(fov, aspect, znear, zfar, matrix);
        return matrix;
    }
    Matrix.perspectiveFovLH = perspectiveFovLH;
    /**
     * Stores a left-handed perspective projection into a given matrix
     * @param fov - defines the horizontal field of view
     * @param aspect - defines the aspect ratio
     * @param znear - defines the near clip plane
     * @param zfar - defines the far clip plane
     * @param result - defines the target matrix
     * @param isVerticalFovFixed - defines it the fov is vertically fixed (default) or horizontally
     */
    function perspectiveFovLHToRef(fov, aspect, znear, zfar, result, isVerticalFovFixed = true) {
        const n = znear;
        const f = zfar;
        const t = 1.0 / Math.tan(fov * 0.5);
        const a = isVerticalFovFixed ? t / aspect : t;
        const b = isVerticalFovFixed ? t : t * aspect;
        const c = (f + n) / (f - n);
        const d = (-2.0 * f * n) / (f - n);
        fromValuesToRef(a, 0.0, 0.0, 0.0, 0.0, b, 0.0, 0.0, 0.0, 0.0, c, 1.0, 0.0, 0.0, d, 0.0, result);
        _updateIdentityStatus(result, false);
    }
    Matrix.perspectiveFovLHToRef = perspectiveFovLHToRef;
    /**
     * Creates a right-handed perspective projection matrix
     * @param fov - defines the horizontal field of view
     * @param aspect - defines the aspect ratio
     * @param znear - defines the near clip plane
     * @param zfar - defines the far clip plane
     * @returns a new matrix as a right-handed perspective projection matrix
     */
    function PerspectiveFovRH(fov, aspect, znear, zfar) {
        const matrix = create();
        perspectiveFovRHToRef(fov, aspect, znear, zfar, matrix);
        return matrix;
    }
    Matrix.PerspectiveFovRH = PerspectiveFovRH;
    /**
     * Stores a right-handed perspective projection into a given matrix
     * @param fov - defines the horizontal field of view
     * @param aspect - defines the aspect ratio
     * @param znear - defines the near clip plane
     * @param zfar - defines the far clip plane
     * @param result - defines the target matrix
     * @param isVerticalFovFixed - defines it the fov is vertically fixed (default) or horizontally
     */
    function perspectiveFovRHToRef(fov, aspect, znear, zfar, result, isVerticalFovFixed = true) {
        /* alternatively self could be expressed as:
        //    m = PerspectiveFovLHToRef
        //    m[10] *= -1.0;
        //    m[11] *= -1.0;
        */
        const n = znear;
        const f = zfar;
        const t = 1.0 / Math.tan(fov * 0.5);
        const a = isVerticalFovFixed ? t / aspect : t;
        const b = isVerticalFovFixed ? t : t * aspect;
        const c = -(f + n) / (f - n);
        const d = (-2 * f * n) / (f - n);
        fromValuesToRef(a, 0.0, 0.0, 0.0, 0.0, b, 0.0, 0.0, 0.0, 0.0, c, -1.0, 0.0, 0.0, d, 0.0, result);
        _updateIdentityStatus(result, false);
    }
    Matrix.perspectiveFovRHToRef = perspectiveFovRHToRef;
    /**
     * Stores a perspective projection for WebVR info a given matrix
     * @param fov - defines the field of view
     * @param znear - defines the near clip plane
     * @param zfar - defines the far clip plane
     * @param result - defines the target matrix
     * @param rightHanded - defines if the matrix must be in right-handed mode (false by default)
     */
    function perspectiveFovWebVRToRef(fov, znear, zfar, result, rightHanded = false) {
        const rightHandedFactor = rightHanded ? -1 : 1;
        const upTan = Math.tan((fov.upDegrees * Math.PI) / 180.0);
        const downTan = Math.tan((fov.downDegrees * Math.PI) / 180.0);
        const leftTan = Math.tan((fov.leftDegrees * Math.PI) / 180.0);
        const rightTan = Math.tan((fov.rightDegrees * Math.PI) / 180.0);
        const xScale = 2.0 / (leftTan + rightTan);
        const yScale = 2.0 / (upTan + downTan);
        const m = result._m;
        m[0] = xScale;
        m[1] = m[2] = m[3] = m[4] = 0.0;
        m[5] = yScale;
        m[6] = m[7] = 0.0;
        m[8] = (leftTan - rightTan) * xScale * 0.5;
        m[9] = -((upTan - downTan) * yScale * 0.5);
        m[10] = -zfar / (znear - zfar);
        m[11] = 1.0 * rightHandedFactor;
        m[12] = m[13] = m[15] = 0.0;
        m[14] = -(2.0 * zfar * znear) / (zfar - znear);
        _markAsUpdated(result);
    }
    Matrix.perspectiveFovWebVRToRef = perspectiveFovWebVRToRef;
    /**
     * Extracts a 2x2 matrix from a given matrix and store the result in a FloatArray
     * @param matrix - defines the matrix to use
     * @returns a new FloatArray array with 4 elements : the 2x2 matrix extracted from the given matrix
     */
    function GetAsMatrix2x2(matrix) {
        return [matrix._m[0], matrix._m[1], matrix._m[4], matrix._m[5]];
    }
    Matrix.GetAsMatrix2x2 = GetAsMatrix2x2;
    /**
     * Extracts a 3x3 matrix from a given matrix and store the result in a FloatArray
     * @param matrix - defines the matrix to use
     * @returns a new FloatArray array with 9 elements : the 3x3 matrix extracted from the given matrix
     */
    function GetAsMatrix3x3(matrix) {
        return [
            matrix._m[0],
            matrix._m[1],
            matrix._m[2],
            matrix._m[4],
            matrix._m[5],
            matrix._m[6],
            matrix._m[8],
            matrix._m[9],
            matrix._m[10]
        ];
    }
    Matrix.GetAsMatrix3x3 = GetAsMatrix3x3;
    /**
     * Compute the transpose of a given matrix
     * @param matrix - defines the matrix to transpose
     * @returns the new matrix
     */
    function transpose(matrix) {
        const result = create();
        transposeToRef(matrix, result);
        return result;
    }
    Matrix.transpose = transpose;
    /**
     * Compute the transpose of a matrix and store it in a target matrix
     * @param matrix - defines the matrix to transpose
     * @param result - defines the target matrix
     */
    function transposeToRef(matrix, result) {
        const rm = result._m;
        const mm = matrix._m;
        rm[0] = mm[0];
        rm[1] = mm[4];
        rm[2] = mm[8];
        rm[3] = mm[12];
        rm[4] = mm[1];
        rm[5] = mm[5];
        rm[6] = mm[9];
        rm[7] = mm[13];
        rm[8] = mm[2];
        rm[9] = mm[6];
        rm[10] = mm[10];
        rm[11] = mm[14];
        rm[12] = mm[3];
        rm[13] = mm[7];
        rm[14] = mm[11];
        rm[15] = mm[15];
        // identity-ness does not change when transposing
        _updateIdentityStatus(result, matrix.isIdentity, matrix._isIdentityDirty);
    }
    Matrix.transposeToRef = transposeToRef;
    /**
     * Computes a reflection matrix from a plane
     * @param plane - defines the reflection plane
     * @returns a new matrix
     */
    function reflection(plane) {
        const matrix = create();
        reflectionToRef(plane, matrix);
        return matrix;
    }
    Matrix.reflection = reflection;
    /**
     * Computes a reflection matrix from a plane
     * @param plane - defines the reflection plane
     * @param result - defines the target matrix
     */
    function reflectionToRef(_plane, result) {
        const plane = Plane.normalize(_plane);
        const x = plane.normal.x;
        const y = plane.normal.y;
        const z = plane.normal.z;
        const temp = -2 * x;
        const temp2 = -2 * y;
        const temp3 = -2 * z;
        fromValuesToRef(temp * x + 1, temp2 * x, temp3 * x, 0.0, temp * y, temp2 * y + 1, temp3 * y, 0.0, temp * z, temp2 * z, temp3 * z + 1, 0.0, temp * plane.d, temp2 * plane.d, temp3 * plane.d, 1.0, result);
    }
    Matrix.reflectionToRef = reflectionToRef;
    /**
     * Sets the given matrix as a rotation matrix composed from the 3 left handed axes
     * @param xaxis - defines the value of the 1st axis
     * @param yaxis - defines the value of the 2nd axis
     * @param zaxis - defines the value of the 3rd axis
     * @param result - defines the target matrix
     */
    function fromXYZAxesToRef(xaxis, yaxis, zaxis, result) {
        fromValuesToRef(xaxis.x, xaxis.y, xaxis.z, 0.0, yaxis.x, yaxis.y, yaxis.z, 0.0, zaxis.x, zaxis.y, zaxis.z, 0.0, 0.0, 0.0, 0.0, 1.0, result);
    }
    Matrix.fromXYZAxesToRef = fromXYZAxesToRef;
    /**
     * Creates a rotation matrix from a quaternion and stores it in a target matrix
     * @param quat - defines the quaternion to use
     * @param result - defines the target matrix
     */
    function fromQuaternionToRef(quat, result) {
        const xx = quat.x * quat.x;
        const yy = quat.y * quat.y;
        const zz = quat.z * quat.z;
        const xy = quat.x * quat.y;
        const zw = quat.z * quat.w;
        const zx = quat.z * quat.x;
        const yw = quat.y * quat.w;
        const yz = quat.y * quat.z;
        const xw = quat.x * quat.w;
        result._m[0] = 1.0 - 2.0 * (yy + zz);
        result._m[1] = 2.0 * (xy + zw);
        result._m[2] = 2.0 * (zx - yw);
        result._m[3] = 0.0;
        result._m[4] = 2.0 * (xy - zw);
        result._m[5] = 1.0 - 2.0 * (zz + xx);
        result._m[6] = 2.0 * (yz + xw);
        result._m[7] = 0.0;
        result._m[8] = 2.0 * (zx + yw);
        result._m[9] = 2.0 * (yz - xw);
        result._m[10] = 1.0 - 2.0 * (yy + xx);
        result._m[11] = 0.0;
        result._m[12] = 0.0;
        result._m[13] = 0.0;
        result._m[14] = 0.0;
        result._m[15] = 1.0;
        _markAsUpdated(result);
    }
    Matrix.fromQuaternionToRef = fromQuaternionToRef;
    /** @internal */
    function _markAsUpdated(self) {
        self.updateFlag = _updateFlagSeed++;
        self.isIdentity = false;
        self.isIdentity3x2 = false;
        self._isIdentityDirty = true;
        self._isIdentity3x2Dirty = true;
    }
    // Properties
    /**
     * Check if the current matrix is identity
     * @returns true is the matrix is the identity matrix
     */
    function isIdentityUpdate(self) {
        if (self._isIdentityDirty) {
            self._isIdentityDirty = false;
            const m = self._m;
            self.isIdentity =
                m[0] === 1.0 &&
                    m[1] === 0.0 &&
                    m[2] === 0.0 &&
                    m[3] === 0.0 &&
                    m[4] === 0.0 &&
                    m[5] === 1.0 &&
                    m[6] === 0.0 &&
                    m[7] === 0.0 &&
                    m[8] === 0.0 &&
                    m[9] === 0.0 &&
                    m[10] === 1.0 &&
                    m[11] === 0.0 &&
                    m[12] === 0.0 &&
                    m[13] === 0.0 &&
                    m[14] === 0.0 &&
                    m[15] === 1.0;
        }
        return self.isIdentity;
    }
    Matrix.isIdentityUpdate = isIdentityUpdate;
    /**
     * Check if the current matrix is identity as a texture matrix (3x2 store in 4x4)
     * @returns true is the matrix is the identity matrix
     */
    function isIdentityAs3x2Update(self) {
        if (self._isIdentity3x2Dirty) {
            self._isIdentity3x2Dirty = false;
            if (self._m[0] !== 1.0 || self._m[5] !== 1.0 || self._m[15] !== 1.0) {
                self.isIdentity3x2 = false;
            }
            else if (self._m[1] !== 0.0 ||
                self._m[2] !== 0.0 ||
                self._m[3] !== 0.0 ||
                self._m[4] !== 0.0 ||
                self._m[6] !== 0.0 ||
                self._m[7] !== 0.0 ||
                self._m[8] !== 0.0 ||
                self._m[9] !== 0.0 ||
                self._m[10] !== 0.0 ||
                self._m[11] !== 0.0 ||
                self._m[12] !== 0.0 ||
                self._m[13] !== 0.0 ||
                self._m[14] !== 0.0) {
                self.isIdentity3x2 = false;
            }
            else {
                self.isIdentity3x2 = true;
            }
        }
        return self.isIdentity3x2;
    }
    Matrix.isIdentityAs3x2Update = isIdentityAs3x2Update;
    /**
     * Gets the determinant of the matrix
     * @returns the matrix determinant
     */
    function determinant(self) {
        if (self.isIdentity === true) {
            return 1;
        }
        const m = self._m;
        // tslint:disable-next-line:one-variable-per-declaration
        const m00 = m[0], m01 = m[1], m02 = m[2], m03 = m[3];
        // tslint:disable-next-line:one-variable-per-declaration
        const m10 = m[4], m11 = m[5], m12 = m[6], m13 = m[7];
        // tslint:disable-next-line:one-variable-per-declaration
        const m20 = m[8], m21 = m[9], m22 = m[10], m23 = m[11];
        // tslint:disable-next-line:one-variable-per-declaration
        const m30 = m[12], m31 = m[13], m32 = m[14], m33 = m[15];
        /*
        // https://en.wikipedia.org/wiki/Laplace_expansion
        // to compute the deterrminant of a 4x4 Matrix we compute the cofactors of any row or column,
        // then we multiply each Cofactor by its corresponding matrix value and sum them all to get the determinant
        // Cofactor(i, j) = sign(i,j) * det(Minor(i, j))
        // where
        //  - sign(i,j) = (i+j) % 2 === 0 ? 1 : -1
        //  - Minor(i, j) is the 3x3 matrix we get by removing row i and column j from current Matrix
        //
        // Here we do that for the 1st row.
        */
        // tslint:disable:variable-name
        const det_22_33 = m22 * m33 - m32 * m23;
        const det_21_33 = m21 * m33 - m31 * m23;
        const det_21_32 = m21 * m32 - m31 * m22;
        const det_20_33 = m20 * m33 - m30 * m23;
        const det_20_32 = m20 * m32 - m22 * m30;
        const det_20_31 = m20 * m31 - m30 * m21;
        const cofact_00 = +(m11 * det_22_33 - m12 * det_21_33 + m13 * det_21_32);
        const cofact_01 = -(m10 * det_22_33 - m12 * det_20_33 + m13 * det_20_32);
        const cofact_02 = +(m10 * det_21_33 - m11 * det_20_33 + m13 * det_20_31);
        const cofact_03 = -(m10 * det_21_32 - m11 * det_20_32 + m12 * det_20_31);
        // tslint:enable:variable-name
        return m00 * cofact_00 + m01 * cofact_01 + m02 * cofact_02 + m03 * cofact_03;
    }
    Matrix.determinant = determinant;
    // Methods
    /**
     * Returns the matrix as a FloatArray
     * @returns the matrix underlying array
     */
    function toArray(self) {
        return self._m;
    }
    Matrix.toArray = toArray;
    /**
     * Returns the matrix as a FloatArray
     * @returns the matrix underlying array.
     */
    function asArray(self) {
        return self._m;
    }
    Matrix.asArray = asArray;
    /**
     * Sets all the matrix elements to zero
     * @returns the current matrix
     */
    function reset(self) {
        fromValuesToRef(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, self);
        _updateIdentityStatus(self, false);
    }
    Matrix.reset = reset;
    /**
     * Adds the current matrix with a second one
     * @param other - defines the matrix to add
     * @returns a new matrix as the addition of the current matrix and the given one
     */
    function add(self, other) {
        const result = create();
        addToRef(self, other, result);
        return result;
    }
    Matrix.add = add;
    /**
     * Sets the given matrix "result" to the addition of the current matrix and the given one
     * @param other - defines the matrix to add
     * @param result - defines the target matrix
     * @returns the current matrix
     */
    function addToRef(self, other, result) {
        for (let index = 0; index < 16; index++) {
            result._m[index] = self._m[index] + other._m[index];
        }
        _markAsUpdated(result);
    }
    Matrix.addToRef = addToRef;
    /**
     * Adds in place the given matrix to the current matrix
     * @param other - defines the second operand
     * @returns the current updated matrix
     */
    function addToSelf(self, other) {
        for (let index = 0; index < 16; index++) {
            self._m[index] += other._m[index];
        }
        _markAsUpdated(self);
    }
    Matrix.addToSelf = addToSelf;
    /**
     * Creates a new matrix as the invert of a given matrix
     * @param source - defines the source matrix
     * @returns the new matrix
     */
    function invert(source) {
        const result = create();
        invertToRef(source, result);
        return result;
    }
    Matrix.invert = invert;
    /**
     * Sets the given matrix to the current inverted Matrix
     * @param other - defines the target matrix
     * @returns the unmodified current matrix
     */
    function invertToRef(source, result) {
        if (source.isIdentity === true) {
            copy(source, result);
            return;
        }
        // the inverse of a Matrix is the transpose of cofactor matrix divided by the determinant
        const m = source._m;
        // tslint:disable:one-variable-per-declaration
        const m00 = m[0], m01 = m[1], m02 = m[2], m03 = m[3];
        const m10 = m[4], m11 = m[5], m12 = m[6], m13 = m[7];
        const m20 = m[8], m21 = m[9], m22 = m[10], m23 = m[11];
        const m30 = m[12], m31 = m[13], m32 = m[14], m33 = m[15];
        // tslint:enable:one-variable-per-declaration
        // tslint:disable:variable-name
        const det_22_33 = m22 * m33 - m32 * m23;
        const det_21_33 = m21 * m33 - m31 * m23;
        const det_21_32 = m21 * m32 - m31 * m22;
        const det_20_33 = m20 * m33 - m30 * m23;
        const det_20_32 = m20 * m32 - m22 * m30;
        const det_20_31 = m20 * m31 - m30 * m21;
        const cofact_00 = +(m11 * det_22_33 - m12 * det_21_33 + m13 * det_21_32);
        const cofact_01 = -(m10 * det_22_33 - m12 * det_20_33 + m13 * det_20_32);
        const cofact_02 = +(m10 * det_21_33 - m11 * det_20_33 + m13 * det_20_31);
        const cofact_03 = -(m10 * det_21_32 - m11 * det_20_32 + m12 * det_20_31);
        const det = m00 * cofact_00 + m01 * cofact_01 + m02 * cofact_02 + m03 * cofact_03;
        if (det === 0) {
            copy(source, result);
            return;
        }
        const detInv = 1 / det;
        const det_12_33 = m12 * m33 - m32 * m13;
        const det_11_33 = m11 * m33 - m31 * m13;
        const det_11_32 = m11 * m32 - m31 * m12;
        const det_10_33 = m10 * m33 - m30 * m13;
        const det_10_32 = m10 * m32 - m30 * m12;
        const det_10_31 = m10 * m31 - m30 * m11;
        const det_12_23 = m12 * m23 - m22 * m13;
        const det_11_23 = m11 * m23 - m21 * m13;
        const det_11_22 = m11 * m22 - m21 * m12;
        const det_10_23 = m10 * m23 - m20 * m13;
        const det_10_22 = m10 * m22 - m20 * m12;
        const det_10_21 = m10 * m21 - m20 * m11;
        const cofact_10 = -(m01 * det_22_33 - m02 * det_21_33 + m03 * det_21_32);
        const cofact_11 = +(m00 * det_22_33 - m02 * det_20_33 + m03 * det_20_32);
        const cofact_12 = -(m00 * det_21_33 - m01 * det_20_33 + m03 * det_20_31);
        const cofact_13 = +(m00 * det_21_32 - m01 * det_20_32 + m02 * det_20_31);
        const cofact_20 = +(m01 * det_12_33 - m02 * det_11_33 + m03 * det_11_32);
        const cofact_21 = -(m00 * det_12_33 - m02 * det_10_33 + m03 * det_10_32);
        const cofact_22 = +(m00 * det_11_33 - m01 * det_10_33 + m03 * det_10_31);
        const cofact_23 = -(m00 * det_11_32 - m01 * det_10_32 + m02 * det_10_31);
        const cofact_30 = -(m01 * det_12_23 - m02 * det_11_23 + m03 * det_11_22);
        const cofact_31 = +(m00 * det_12_23 - m02 * det_10_23 + m03 * det_10_22);
        const cofact_32 = -(m00 * det_11_23 - m01 * det_10_23 + m03 * det_10_21);
        const cofact_33 = +(m00 * det_11_22 - m01 * det_10_22 + m02 * det_10_21);
        fromValuesToRef(cofact_00 * detInv, cofact_10 * detInv, cofact_20 * detInv, cofact_30 * detInv, cofact_01 * detInv, cofact_11 * detInv, cofact_21 * detInv, cofact_31 * detInv, cofact_02 * detInv, cofact_12 * detInv, cofact_22 * detInv, cofact_32 * detInv, cofact_03 * detInv, cofact_13 * detInv, cofact_23 * detInv, cofact_33 * detInv, result);
        // tslint:enable:variable-name
    }
    Matrix.invertToRef = invertToRef;
    /**
     * add a value at the specified position in the current Matrix
     * @param index - the index of the value within the matrix. between 0 and 15.
     * @param value - the value to be added
     * @returns the current updated matrix
     */
    function addAtIndex(self, index, value) {
        self._m[index] += value;
        _markAsUpdated(self);
    }
    Matrix.addAtIndex = addAtIndex;
    /**
     * mutiply the specified position in the current Matrix by a value
     * @param index - the index of the value within the matrix. between 0 and 15.
     * @param value - the value to be added
     * @returns the current updated matrix
     */
    function multiplyAtIndex(self, index, value) {
        self._m[index] *= value;
        _markAsUpdated(self);
        return self;
    }
    Matrix.multiplyAtIndex = multiplyAtIndex;
    /**
     * Inserts the translation vector (using 3 floats) in the current matrix
     * @param x - defines the 1st component of the translation
     * @param y - defines the 2nd component of the translation
     * @param z - defines the 3rd component of the translation
     * @returns the current updated matrix
     */
    function setTranslationFromFloats(self, x, y, z) {
        self._m[12] = x;
        self._m[13] = y;
        self._m[14] = z;
        _markAsUpdated(self);
    }
    Matrix.setTranslationFromFloats = setTranslationFromFloats;
    /**
     * Inserts the translation vector in the current matrix
     * @param vector3 - defines the translation to insert
     * @returns the current updated matrix
     */
    function setTranslation(self, vector3) {
        setTranslationFromFloats(self, vector3.x, vector3.y, vector3.z);
    }
    Matrix.setTranslation = setTranslation;
    /**
     * Gets the translation value of the current matrix
     * @returns a new Vector3 as the extracted translation from the matrix
     */
    function getTranslation(self) {
        return Vector3.create(self._m[12], self._m[13], self._m[14]);
    }
    Matrix.getTranslation = getTranslation;
    /**
     * Fill a Vector3 with the extracted translation from the matrix
     * @param result - defines the Vector3 where to store the translation
     * @returns the current matrix
     */
    function getTranslationToRef(self, result) {
        result.x = self._m[12];
        result.y = self._m[13];
        result.z = self._m[14];
    }
    Matrix.getTranslationToRef = getTranslationToRef;
    /**
     * Remove rotation and scaling part from the matrix
     * @returns the updated matrix
     */
    function removeRotationAndScaling(self) {
        const m = self._m;
        fromValuesToRef(1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, m[12], m[13], m[14], m[15], self);
        _updateIdentityStatus(self, m[12] === 0 && m[13] === 0 && m[14] === 0 && m[15] === 1);
        return self;
    }
    Matrix.removeRotationAndScaling = removeRotationAndScaling;
    /**
     * Multiply two matrices
     * @param other - defines the second operand
     * @returns a new matrix set with the multiplication result of the current Matrix and the given one
     */
    function multiply(self, other) {
        const result = create();
        multiplyToRef(self, other, result);
        return result;
    }
    Matrix.multiply = multiply;
    /**
     * Copy the current matrix from the given one
     * @param other - defines the source matrix
     * @returns the current updated matrix
     */
    function copy(from, dest) {
        copyToArray(from, dest._m);
        _updateIdentityStatus(dest, from.isIdentity, from._isIdentityDirty, from.isIdentity3x2, from._isIdentity3x2Dirty);
    }
    Matrix.copy = copy;
    /**
     * Populates the given array from the starting index with the current matrix values
     * @param array - defines the target array
     * @param offset - defines the offset in the target array where to start storing values
     * @returns the current matrix
     */
    function copyToArray(self, arrayDest, offsetDest = 0) {
        for (let index = 0; index < 16; index++) {
            arrayDest[offsetDest + index] = self._m[index];
        }
    }
    Matrix.copyToArray = copyToArray;
    /**
     * Sets the given matrix "result" with the multiplication result of the current Matrix and the given one
     * @param other - defines the second operand
     * @param result - defines the matrix where to store the multiplication
     * @returns the current matrix
     */
    function multiplyToRef(self, other, result) {
        if (self.isIdentity) {
            copy(other, result);
            return;
        }
        if (other.isIdentity) {
            copy(self, result);
            return;
        }
        multiplyToArray(self, other, result._m, 0);
        _markAsUpdated(result);
    }
    Matrix.multiplyToRef = multiplyToRef;
    /**
     * Sets the FloatArray "result" from the given index "offset" with the multiplication of the current matrix and the given one
     * @param other - defines the second operand
     * @param result - defines the array where to store the multiplication
     * @param offset - defines the offset in the target array where to start storing values
     * @returns the current matrix
     */
    function multiplyToArray(self, other, result, offset) {
        const m = self._m;
        const otherM = other._m;
        // tslint:disable:one-variable-per-declaration
        const tm0 = m[0], tm1 = m[1], tm2 = m[2], tm3 = m[3];
        const tm4 = m[4], tm5 = m[5], tm6 = m[6], tm7 = m[7];
        const tm8 = m[8], tm9 = m[9], tm10 = m[10], tm11 = m[11];
        const tm12 = m[12], tm13 = m[13], tm14 = m[14], tm15 = m[15];
        const om0 = otherM[0], om1 = otherM[1], om2 = otherM[2], om3 = otherM[3];
        const om4 = otherM[4], om5 = otherM[5], om6 = otherM[6], om7 = otherM[7];
        const om8 = otherM[8], om9 = otherM[9], om10 = otherM[10], om11 = otherM[11];
        const om12 = otherM[12], om13 = otherM[13], om14 = otherM[14], om15 = otherM[15];
        // tslint:enable:one-variable-per-declaration
        result[offset] = tm0 * om0 + tm1 * om4 + tm2 * om8 + tm3 * om12;
        result[offset + 1] = tm0 * om1 + tm1 * om5 + tm2 * om9 + tm3 * om13;
        result[offset + 2] = tm0 * om2 + tm1 * om6 + tm2 * om10 + tm3 * om14;
        result[offset + 3] = tm0 * om3 + tm1 * om7 + tm2 * om11 + tm3 * om15;
        result[offset + 4] = tm4 * om0 + tm5 * om4 + tm6 * om8 + tm7 * om12;
        result[offset + 5] = tm4 * om1 + tm5 * om5 + tm6 * om9 + tm7 * om13;
        result[offset + 6] = tm4 * om2 + tm5 * om6 + tm6 * om10 + tm7 * om14;
        result[offset + 7] = tm4 * om3 + tm5 * om7 + tm6 * om11 + tm7 * om15;
        result[offset + 8] = tm8 * om0 + tm9 * om4 + tm10 * om8 + tm11 * om12;
        result[offset + 9] = tm8 * om1 + tm9 * om5 + tm10 * om9 + tm11 * om13;
        result[offset + 10] = tm8 * om2 + tm9 * om6 + tm10 * om10 + tm11 * om14;
        result[offset + 11] = tm8 * om3 + tm9 * om7 + tm10 * om11 + tm11 * om15;
        result[offset + 12] = tm12 * om0 + tm13 * om4 + tm14 * om8 + tm15 * om12;
        result[offset + 13] = tm12 * om1 + tm13 * om5 + tm14 * om9 + tm15 * om13;
        result[offset + 14] = tm12 * om2 + tm13 * om6 + tm14 * om10 + tm15 * om14;
        result[offset + 15] = tm12 * om3 + tm13 * om7 + tm14 * om11 + tm15 * om15;
    }
    Matrix.multiplyToArray = multiplyToArray;
    /**
     * Check equality between self matrix and a second one
     * @param value - defines the second matrix to compare
     * @returns true is the current matrix and the given one values are strictly equal
     */
    function equals(self, value) {
        const other = value;
        if (!other) {
            return false;
        }
        if (self.isIdentity || other.isIdentity) {
            if (!self._isIdentityDirty && !other._isIdentityDirty) {
                return self.isIdentity && other.isIdentity;
            }
        }
        const m = self._m;
        const om = other._m;
        return (m[0] === om[0] &&
            m[1] === om[1] &&
            m[2] === om[2] &&
            m[3] === om[3] &&
            m[4] === om[4] &&
            m[5] === om[5] &&
            m[6] === om[6] &&
            m[7] === om[7] &&
            m[8] === om[8] &&
            m[9] === om[9] &&
            m[10] === om[10] &&
            m[11] === om[11] &&
            m[12] === om[12] &&
            m[13] === om[13] &&
            m[14] === om[14] &&
            m[15] === om[15]);
    }
    Matrix.equals = equals;
    /**
     * Clone the current matrix
     * @returns a new matrix from the current matrix
     */
    function clone(self) {
        const result = create();
        copy(self, result);
        return result;
    }
    Matrix.clone = clone;
    /**
     * Gets the hash code of the current matrix
     * @returns the hash code
     */
    function getHashCode(self) {
        let hash = self._m[0] || 0;
        for (let i = 1; i < 16; i++) {
            hash = (hash * 397) ^ (self._m[i] || 0);
        }
        return hash;
    }
    Matrix.getHashCode = getHashCode;
    /**
     * Decomposes the current Matrix into a translation, rotation and scaling components
     * @param scale - defines the scale vector3 given as a reference to update
     * @param rotation - defines the rotation quaternion given as a reference to update
     * @param translation - defines the translation vector3 given as a reference to update
     * @returns true if operation was successful
     */
    function decompose(self, scale, rotation, translation) {
        if (self.isIdentity) {
            if (translation) {
                translation = Vector3.create(0, 0, 0);
            }
            if (scale) {
                scale = Vector3.create(0, 0, 0);
            }
            if (rotation) {
                rotation = Quaternion.create(0, 0, 0, 1);
            }
            return true;
        }
        const m = self._m;
        if (translation) {
            translation = Vector3.create(m[12], m[13], m[14]);
        }
        const usedScale = scale || Vector3.Zero();
        usedScale.x = Math.sqrt(m[0] * m[0] + m[1] * m[1] + m[2] * m[2]);
        usedScale.y = Math.sqrt(m[4] * m[4] + m[5] * m[5] + m[6] * m[6]);
        usedScale.z = Math.sqrt(m[8] * m[8] + m[9] * m[9] + m[10] * m[10]);
        if (determinant(self) <= 0) {
            usedScale.y *= -1;
        }
        if (usedScale.x === 0 || usedScale.y === 0 || usedScale.z === 0) {
            if (rotation) {
                rotation = Quaternion.create(0, 0, 0, 1);
            }
            return false;
        }
        if (rotation) {
            // tslint:disable-next-line:one-variable-per-declaration
            const sx = 1 / usedScale.x, sy = 1 / usedScale.y, sz = 1 / usedScale.z;
            const tmpMatrix = create();
            fromValuesToRef(m[0] * sx, m[1] * sx, m[2] * sx, 0.0, m[4] * sy, m[5] * sy, m[6] * sy, 0.0, m[8] * sz, m[9] * sz, m[10] * sz, 0.0, 0.0, 0.0, 0.0, 1.0, tmpMatrix);
            Quaternion.fromRotationMatrixToRef(tmpMatrix, rotation);
        }
        return true;
    }
    Matrix.decompose = decompose;
    /**
     * Gets specific row of the matrix
     * @param index - defines the number of the row to get
     * @returns the index-th row of the current matrix as a new Vector4
     */
    // TODO
    // export function getRow(index: number): Nullable<Vector4> {
    //   if (index < 0 || index > 3) {
    //     return null
    //   }
    //   const i = index * 4
    //   return new Vector4(
    //     self._m[i + 0],
    //     self._m[i + 1],
    //     self._m[i + 2],
    //     self._m[i + 3]
    //   )
    // }
    /**
     * Sets the index-th row of the current matrix to the vector4 values
     * @param index - defines the number of the row to set
     * @param row - defines the target vector4
     * @returns the updated current matrix
     */
    // TODO
    // export function setRow(index: number, row: Vector4): MutableMatrix {
    //   return setRowFromFloats(index, row.x, row.y, row.z, row.w)
    // }
    /**
     * Sets the index-th row of the current matrix with the given 4 x float values
     * @param index - defines the row index
     * @param x - defines the x component to set
     * @param y - defines the y component to set
     * @param z - defines the z component to set
     * @param w - defines the w component to set
     * @returns the updated current matrix
     */
    function setRowFromFloats(self, index, x, y, z, w) {
        if (index < 0 || index > 3) {
            return;
        }
        const i = index * 4;
        self._m[i + 0] = x;
        self._m[i + 1] = y;
        self._m[i + 2] = z;
        self._m[i + 3] = w;
        _markAsUpdated(self);
    }
    Matrix.setRowFromFloats = setRowFromFloats;
    /**
     * Compute a new matrix set with the current matrix values multiplied by scale (float)
     * @param scale - defines the scale factor
     * @returns a new matrix
     */
    function scale(self, scale) {
        const result = create();
        scaleToRef(self, scale, result);
        return result;
    }
    Matrix.scale = scale;
    /**
     * Scale the current matrix values by a factor to a given result matrix
     * @param scale - defines the scale factor
     * @param result - defines the matrix to store the result
     * @returns the current matrix
     */
    function scaleToRef(self, scale, result) {
        for (let index = 0; index < 16; index++) {
            result._m[index] = self._m[index] * scale;
        }
        _markAsUpdated(result);
    }
    Matrix.scaleToRef = scaleToRef;
    /**
     * Scale the current matrix values by a factor and add the result to a given matrix
     * @param scale - defines the scale factor
     * @param result - defines the Matrix to store the result
     * @returns the current matrix
     */
    function scaleAndAddToRef(self, scale, result) {
        for (let index = 0; index < 16; index++) {
            result._m[index] += self._m[index] * scale;
        }
        _markAsUpdated(result);
    }
    Matrix.scaleAndAddToRef = scaleAndAddToRef;
    /**
     * Writes to the given matrix a normal matrix, computed from self one (using values from identity matrix for fourth row and column).
     * @param ref - matrix to store the result
     */
    function normalMatrixToRef(self, ref) {
        const tmp = create();
        invertToRef(self, tmp);
        transposeToRef(tmp, ref);
        const m = ref._m;
        fromValuesToRef(m[0], m[1], m[2], 0.0, m[4], m[5], m[6], 0.0, m[8], m[9], m[10], 0.0, 0.0, 0.0, 0.0, 1.0, ref);
    }
    Matrix.normalMatrixToRef = normalMatrixToRef;
    /**
     * Gets only rotation part of the current matrix
     * @returns a new matrix sets to the extracted rotation matrix from the current one
     */
    function getRotationMatrix(self) {
        const result = create();
        getRotationMatrixToRef(self, result);
        return result;
    }
    Matrix.getRotationMatrix = getRotationMatrix;
    /**
     * Extracts the rotation matrix from the current one and sets it as the given "result"
     * @param result - defines the target matrix to store data to
     * @returns the current matrix
     */
    function getRotationMatrixToRef(self, result) {
        const scale = Vector3.Zero();
        if (!decompose(self, scale)) {
            result = Identity();
            return;
        }
        const m = self._m;
        // tslint:disable-next-line:one-variable-per-declaration
        const sx = 1 / scale.x, sy = 1 / scale.y, sz = 1 / scale.z;
        fromValuesToRef(m[0] * sx, m[1] * sx, m[2] * sx, 0.0, m[4] * sy, m[5] * sy, m[6] * sy, 0.0, m[8] * sz, m[9] * sz, m[10] * sz, 0.0, 0.0, 0.0, 0.0, 1.0, result);
    }
    Matrix.getRotationMatrixToRef = getRotationMatrixToRef;
    /**
     * Toggles model matrix from being right handed to left handed in place and vice versa
     */
    function toggleModelMatrixHandInPlace(self) {
        self._m[2] *= -1;
        self._m[6] *= -1;
        self._m[8] *= -1;
        self._m[9] *= -1;
        self._m[14] *= -1;
        _markAsUpdated(self);
    }
    Matrix.toggleModelMatrixHandInPlace = toggleModelMatrixHandInPlace;
    /**
     * Toggles projection matrix from being right handed to left handed in place and vice versa
     */
    function toggleProjectionMatrixHandInPlace(self) {
        self._m[8] *= -1;
        self._m[9] *= -1;
        self._m[10] *= -1;
        self._m[11] *= -1;
        _markAsUpdated(self);
    }
    Matrix.toggleProjectionMatrixHandInPlace = toggleProjectionMatrixHandInPlace;
    /** @internal */
    function _updateIdentityStatus(self, isIdentity, isIdentityDirty = false, isIdentity3x2 = false, isIdentity3x2Dirty = true) {
        self.updateFlag = _updateFlagSeed++;
        self.isIdentity = isIdentity;
        self.isIdentity3x2 = isIdentity || isIdentity3x2;
        self._isIdentityDirty = self.isIdentity ? false : isIdentityDirty;
        self._isIdentity3x2Dirty = self.isIdentity3x2 ? false : isIdentity3x2Dirty;
    }
})(Matrix || (Matrix = {}));

/**
 * @public
 * Quaternion is a type and a namespace.
 * ```
 * // The namespace contains all types and functions to operates with Quaternion
 * const next = Quaternion.add(pointA, velocityA)
 * // The type Quaternion is an alias to Quaternion.ReadonlyQuaternion
 * const readonlyRotation: Quaternion = Quaternion.Zero()
 * readonlyRotation.x = 0.1 // this FAILS
 *
 * // For mutable usage, use `Quaternion.Mutable`
 * const rotation: Quaternion.Mutable = Quaternion.Identity()
 * rotation.x = 3.0 // this WORKS
 * ```
 */
var Quaternion;
(function (Quaternion) {
    /**
     * Creates a new Quaternion from the given floats
     * @param x - defines the first component (0 by default)
     * @param y - defines the second component (0 by default)
     * @param z - defines the third component (0 by default)
     * @param w - defines the fourth component (1.0 by default)
     */
    function create(
    /** defines the first component (0 by default) */
    x = 0.0, 
    /** defines the second component (0 by default) */
    y = 0.0, 
    /** defines the third component (0 by default) */
    z = 0.0, 
    /** defines the fourth component (1.0 by default) */
    w = 1.0) {
        return { x, y, z, w };
    }
    Quaternion.create = create;
    /**
     * Returns a new Quaternion as the result of the addition of the two given quaternions.
     * @param q1 - the first quaternion
     * @param q2 - the second quaternion
     * @returns the resulting quaternion
     */
    function add(q1, q2) {
        return { x: q1.x + q2.x, y: q1.y + q2.y, z: q1.z + q2.z, w: q1.w + q2.w };
    }
    Quaternion.add = add;
    /**
     * Creates a new rotation from the given Euler float angles (y, x, z) and stores it in the target quaternion
     * @param yaw - defines the rotation around Y axis (radians)
     * @param pitch - defines the rotation around X axis (radians)
     * @param roll - defines the rotation around Z axis (radians)
     * @returns result quaternion
     */
    function fromRotationYawPitchRoll(yaw, pitch, roll) {
        // Implemented unity-based calculations from: https://stackoverflow.com/a/56055813
        const halfPitch = pitch * 0.5;
        const halfYaw = yaw * 0.5;
        const halfRoll = roll * 0.5;
        const c1 = Math.cos(halfPitch);
        const c2 = Math.cos(halfYaw);
        const c3 = Math.cos(halfRoll);
        const s1 = Math.sin(halfPitch);
        const s2 = Math.sin(halfYaw);
        const s3 = Math.sin(halfRoll);
        return create(c2 * s1 * c3 + s2 * c1 * s3, s2 * c1 * c3 - c2 * s1 * s3, c2 * c1 * s3 - s2 * s1 * c3, c2 * c1 * c3 + s2 * s1 * s3);
    }
    Quaternion.fromRotationYawPitchRoll = fromRotationYawPitchRoll;
    /**
     * Returns a rotation that rotates z degrees around the z axis, x degrees around the x axis, and y degrees around the y axis.
     * @param x - the rotation on the x axis in euler degrees
     * @param y - the rotation on the y axis in euler degrees
     * @param z - the rotation on the z axis in euler degrees
     */
    function fromEulerDegrees(x, y, z) {
        return fromRotationYawPitchRoll(y * DEG2RAD, x * DEG2RAD, z * DEG2RAD);
    }
    Quaternion.fromEulerDegrees = fromEulerDegrees;
    /**
     * Gets length of current quaternion
     * @returns the quaternion length (float)
     */
    function length(q) {
        return Math.sqrt(lengthSquared(q));
    }
    Quaternion.length = length;
    /**
     * Gets length of current quaternion
     * @returns the quaternion length (float)
     */
    function lengthSquared(q) {
        return q.x * q.x + q.y * q.y + q.z * q.z + q.w * q.w;
    }
    Quaternion.lengthSquared = lengthSquared;
    /**
     * Returns the dot product (float) between the quaternions "left" and "right"
     * @param left - defines the left operand
     * @param right - defines the right operand
     * @returns the dot product
     */
    function dot(left, right) {
        return (left.x * right.x + left.y * right.y + left.z * right.z + left.w * right.w);
    }
    Quaternion.dot = dot;
    /**
     * Returns the angle in degrees between two rotations a and b.
     * @param quat1 - defines the first quaternion
     * @param quat2 - defines the second quaternion
     * @returns the degrees angle
     */
    function angle(quat1, quat2) {
        const dotVal = dot(quat1, quat2);
        return Math.acos(Math.min(Math.abs(dotVal), 1)) * 2 * RAD2DEG;
    }
    Quaternion.angle = angle;
    /**
     * The from quaternion is rotated towards to by an angular step of maxDegreesDelta.
     * @param from - defines the first quaternion
     * @param to - defines the second quaternion
     * @param maxDegreesDelta - the interval step
     */
    function rotateTowards(from, to, maxDegreesDelta) {
        const num = angle(from, to);
        if (num === 0) {
            return to;
        }
        const t = Math.min(1, maxDegreesDelta / num);
        return slerp(from, to, t);
    }
    Quaternion.rotateTowards = rotateTowards;
    /**
     * Creates a rotation with the specified forward and upwards directions.
     * @param forward - the direction to look in
     * @param up - the vector that defines in which direction up is
     */
    function lookRotation(forward, up = { x: 0.0, y: 1.0, z: 0.0 }) {
        const forwardNew = Vector3.normalize(forward);
        const right = Vector3.normalize(Vector3.cross(up, forwardNew));
        const upNew = Vector3.cross(forwardNew, right);
        const m00 = right.x;
        const m01 = right.y;
        const m02 = right.z;
        const m10 = upNew.x;
        const m11 = upNew.y;
        const m12 = upNew.z;
        const m20 = forwardNew.x;
        const m21 = forwardNew.y;
        const m22 = forwardNew.z;
        const num8 = m00 + m11 + m22;
        const quaternion = create();
        if (num8 > 0) {
            let num = Math.sqrt(num8 + 1);
            quaternion.w = num * 0.5;
            num = 0.5 / num;
            quaternion.x = (m12 - m21) * num;
            quaternion.y = (m20 - m02) * num;
            quaternion.z = (m01 - m10) * num;
            return quaternion;
        }
        if (m00 >= m11 && m00 >= m22) {
            const num7 = Math.sqrt(1 + m00 - m11 - m22);
            const num4 = 0.5 / num7;
            quaternion.x = 0.5 * num7;
            quaternion.y = (m01 + m10) * num4;
            quaternion.z = (m02 + m20) * num4;
            quaternion.w = (m12 - m21) * num4;
            return quaternion;
        }
        if (m11 > m22) {
            const num6 = Math.sqrt(1 + m11 - m00 - m22);
            const num3 = 0.5 / num6;
            quaternion.x = (m10 + m01) * num3;
            quaternion.y = 0.5 * num6;
            quaternion.z = (m21 + m12) * num3;
            quaternion.w = (m20 - m02) * num3;
            return quaternion;
        }
        const num5 = Math.sqrt(1 + m22 - m00 - m11);
        const num2 = 0.5 / num5;
        quaternion.x = (m20 + m02) * num2;
        quaternion.y = (m21 + m12) * num2;
        quaternion.z = 0.5 * num5;
        quaternion.w = (m01 - m10) * num2;
        return quaternion;
    }
    Quaternion.lookRotation = lookRotation;
    /**
     * Normalize in place the current quaternion
     * @returns the current updated quaternion
     */
    function normalize(q) {
        const qLength = 1.0 / length(q);
        return create(q.x * qLength, q.y * qLength, q.z * qLength, q.w * qLength);
    }
    Quaternion.normalize = normalize;
    /**
     * Creates a rotation which rotates from fromDirection to toDirection.
     * @param from - defines the first direction Vector
     * @param to - defines the target direction Vector
     */
    function fromToRotation(from, to, up = Vector3.Up()) {
        // Unity-based calculations implemented from https://forum.unity.com/threads/quaternion-lookrotation-around-an-axis.608470/#post-4069888
        const v0 = Vector3.normalize(from);
        const v1 = Vector3.normalize(to);
        const a = Vector3.cross(v0, v1);
        const w = Math.sqrt(Vector3.lengthSquared(v0) * Vector3.lengthSquared(v1)) +
            Vector3.dot(v0, v1);
        if (Vector3.lengthSquared(a) < 0.0001) {
            // the vectors are parallel, check w to find direction
            // if w is 0 then values are opposite, and we sould rotate 180 degrees around the supplied axis
            // otherwise the vectors in the same direction and no rotation should occur
            return Math.abs(w) < 0.0001
                ? normalize(create(up.x, up.y, up.z, 0))
                : Identity();
        }
        else {
            return normalize(create(a.x, a.y, a.z, w));
        }
    }
    Quaternion.fromToRotation = fromToRotation;
    /**
     * Creates an identity quaternion
     * @returns - the identity quaternion
     */
    function Identity() {
        return create(0.0, 0.0, 0.0, 1.0);
    }
    Quaternion.Identity = Identity;
    /**
     * Gets or sets the euler angle representation of the rotation.
     * Implemented unity-based calculations from: https://stackoverflow.com/a/56055813
     * @public
     * @returns a new Vector3 with euler angles degrees
     */
    function toEulerAngles(q) {
        const out = Vector3.create();
        // if the input quaternion is normalized, this is exactly one. Otherwise, this acts as a correction factor for the quaternion's not-normalizedness
        const unit = q.x * q.x + q.y * q.y + q.z * q.z + q.w * q.w;
        // q will have a magnitude of 0.5 or greater if and only if q is a singularity case
        const test = q.x * q.w - q.y * q.z;
        if (test > 0.4995 * unit) {
            // singularity at north pole
            out.x = Math.PI / 2;
            out.y = 2 * Math.atan2(q.y, q.x);
            out.z = 0;
        }
        else if (test < -0.4995 * unit) {
            // singularity at south pole
            out.x = -Math.PI / 2;
            out.y = -2 * Math.atan2(q.y, q.x);
            out.z = 0;
        }
        else {
            // no singularity - q is the majority of cases
            out.x = Math.asin(2 * (q.w * q.x - q.y * q.z));
            out.y = Math.atan2(2 * q.w * q.y + 2 * q.z * q.x, 1 - 2 * (q.x * q.x + q.y * q.y));
            out.z = Math.atan2(2 * q.w * q.z + 2 * q.x * q.y, 1 - 2 * (q.z * q.z + q.x * q.x));
        }
        out.x *= RAD2DEG;
        out.y *= RAD2DEG;
        out.z *= RAD2DEG;
        // ensure the degree values are between 0 and 360
        out.x = Scalar.repeat(out.x, 360);
        out.y = Scalar.repeat(out.y, 360);
        out.z = Scalar.repeat(out.z, 360);
        return out;
    }
    Quaternion.toEulerAngles = toEulerAngles;
    /**
     * Creates a new rotation from the given Euler float angles (y, x, z) and stores it in the target quaternion
     * @param yaw - defines the rotation around Y axis (radians)
     * @param pitch - defines the rotation around X axis (radians)
     * @param roll - defines the rotation around Z axis (radians)
     * @param result - defines the target quaternion
     */
    function fromRotationYawPitchRollToRef(yaw, pitch, roll, result) {
        // Implemented unity-based calculations from: https://stackoverflow.com/a/56055813
        const halfPitch = pitch * 0.5;
        const halfYaw = yaw * 0.5;
        const halfRoll = roll * 0.5;
        const c1 = Math.cos(halfPitch);
        const c2 = Math.cos(halfYaw);
        const c3 = Math.cos(halfRoll);
        const s1 = Math.sin(halfPitch);
        const s2 = Math.sin(halfYaw);
        const s3 = Math.sin(halfRoll);
        result.x = c2 * s1 * c3 + s2 * c1 * s3;
        result.y = s2 * c1 * c3 - c2 * s1 * s3;
        result.z = c2 * c1 * s3 - s2 * s1 * c3;
        result.w = c2 * c1 * c3 + s2 * s1 * s3;
    }
    Quaternion.fromRotationYawPitchRollToRef = fromRotationYawPitchRollToRef;
    /**
     * Updates the given quaternion with the given rotation matrix values
     * @param matrix - defines the source matrix
     * @param result - defines the target quaternion
     */
    function fromRotationMatrixToRef(matrix, result) {
        const data = matrix._m;
        // tslint:disable:one-variable-per-declaration
        const m11 = data[0], m12 = data[4], m13 = data[8];
        const m21 = data[1], m22 = data[5], m23 = data[9];
        const m31 = data[2], m32 = data[6], m33 = data[10];
        // tslint:enable:one-variable-per-declaration
        const trace = m11 + m22 + m33;
        let s;
        if (trace > 0) {
            s = 0.5 / Math.sqrt(trace + 1.0);
            result.w = 0.25 / s;
            result.x = (m32 - m23) * s;
            result.y = (m13 - m31) * s;
            result.z = (m21 - m12) * s;
        }
        else if (m11 > m22 && m11 > m33) {
            s = 2.0 * Math.sqrt(1.0 + m11 - m22 - m33);
            result.w = (m32 - m23) / s;
            result.x = 0.25 * s;
            result.y = (m12 + m21) / s;
            result.z = (m13 + m31) / s;
        }
        else if (m22 > m33) {
            s = 2.0 * Math.sqrt(1.0 + m22 - m11 - m33);
            result.w = (m13 - m31) / s;
            result.x = (m12 + m21) / s;
            result.y = 0.25 * s;
            result.z = (m23 + m32) / s;
        }
        else {
            s = 2.0 * Math.sqrt(1.0 + m33 - m11 - m22);
            result.w = (m21 - m12) / s;
            result.x = (m13 + m31) / s;
            result.y = (m23 + m32) / s;
            result.z = 0.25 * s;
        }
    }
    Quaternion.fromRotationMatrixToRef = fromRotationMatrixToRef;
    /**
     * Interpolates between two quaternions
     * @param left - defines first quaternion
     * @param right - defines second quaternion
     * @param amount - defines the gradient to use
     * @returns the new interpolated quaternion
     */
    function slerp(left, right, amount) {
        const result = Quaternion.Identity();
        Quaternion.slerpToRef(left, right, amount, result);
        return result;
    }
    Quaternion.slerp = slerp;
    /**
     * Interpolates between two quaternions and stores it into a target quaternion
     * @param left - defines first quaternion
     * @param right - defines second quaternion
     * @param amount - defines the gradient to use
     * @param result - defines the target quaternion
     */
    function slerpToRef(left, right, amount, result) {
        let num2;
        let num3;
        let num4 = left.x * right.x + left.y * right.y + left.z * right.z + left.w * right.w;
        let flag = false;
        if (num4 < 0) {
            flag = true;
            num4 = -num4;
        }
        if (num4 > 0.999999) {
            num3 = 1 - amount;
            num2 = flag ? -amount : amount;
        }
        else {
            const num5 = Math.acos(num4);
            const num6 = 1.0 / Math.sin(num5);
            num3 = Math.sin((1.0 - amount) * num5) * num6;
            num2 = flag
                ? -Math.sin(amount * num5) * num6
                : Math.sin(amount * num5) * num6;
        }
        result.x = num3 * left.x + num2 * right.x;
        result.y = num3 * left.y + num2 * right.y;
        result.z = num3 * left.z + num2 * right.z;
        result.w = num3 * left.w + num2 * right.w;
    }
    Quaternion.slerpToRef = slerpToRef;
    /**
     * Multiplies two quaternions
     * @param self - defines the first operand
     * @param q1 - defines the second operand
     * @returns a new quaternion set as the multiplication result of the self one with the given one "q1"
     */
    function multiply(self, q1) {
        const result = create(0, 0, 0, 1.0);
        multiplyToRef(self, q1, result);
        return result;
    }
    Quaternion.multiply = multiply;
    /**
     * Sets the given "result" as the the multiplication result of the self one with the given one "q1"
     * @param self - defines the first operand
     * @param q1 - defines the second operand
     * @param result - defines the target quaternion
     * @returns the current quaternion
     */
    function multiplyToRef(self, q1, result) {
        result.x = self.x * q1.w + self.y * q1.z - self.z * q1.y + self.w * q1.x;
        result.y = -self.x * q1.z + self.y * q1.w + self.z * q1.x + self.w * q1.y;
        result.z = self.x * q1.y - self.y * q1.x + self.z * q1.w + self.w * q1.z;
        result.w = -self.x * q1.x - self.y * q1.y - self.z * q1.z + self.w * q1.w;
    }
    Quaternion.multiplyToRef = multiplyToRef;
    /**
     *
     * @param degrees - the angle degrees
     * @param axis - vector3
     * @returns a new Quaternion
     */
    function fromAngleAxis(degrees, axis) {
        if (Vector3.lengthSquared(axis) === 0) {
            return Quaternion.Identity();
        }
        const result = Identity();
        let radians = degrees * DEG2RAD;
        radians *= 0.5;
        const a2 = Vector3.normalize(axis);
        Vector3.scaleToRef(a2, Math.sin(radians), a2);
        result.x = a2.x;
        result.y = a2.y;
        result.z = a2.z;
        result.w = Math.cos(radians);
        return normalize(result);
    }
    Quaternion.fromAngleAxis = fromAngleAxis;
    /**
     * Creates a new quaternion containing the rotation value to reach the target (axis1, axis2, axis3) orientation as a rotated XYZ system (axis1, axis2 and axis3 are normalized during this operation)
     * @param axis1 - defines the first axis
     * @param axis2 - defines the second axis
     * @param axis3 - defines the third axis
     * @returns the new quaternion
     */
    function fromAxisToRotationQuaternion(axis1, axis2, axis3) {
        const quat = Quaternion.create(0.0, 0.0, 0.0, 0.0);
        fromAxisToRotationQuaternionToRef(axis1, axis2, axis3, quat);
        return quat;
    }
    Quaternion.fromAxisToRotationQuaternion = fromAxisToRotationQuaternion;
    /**
     * Creates a rotation value to reach the target (axis1, axis2, axis3) orientation as a rotated XYZ system (axis1, axis2 and axis3 are normalized during this operation) and stores it in the target quaternion
     * @param axis1 - defines the first axis
     * @param axis2 - defines the second axis
     * @param axis3 - defines the third axis
     * @param ref - defines the target quaternion
     */
    function fromAxisToRotationQuaternionToRef(axis1, axis2, axis3, ref) {
        const rotMat = Matrix.create();
        Matrix.fromXYZAxesToRef(Vector3.normalize(axis1), Vector3.normalize(axis2), Vector3.normalize(axis3), rotMat);
        Quaternion.fromRotationMatrixToRef(rotMat, ref);
    }
    Quaternion.fromAxisToRotationQuaternionToRef = fromAxisToRotationQuaternionToRef;
    /**
     * Returns a zero filled quaternion
     */
    function Zero() {
        return create(0.0, 0.0, 0.0, 0.0);
    }
    Quaternion.Zero = Zero;
    /**
     * @public
     * Rotates the transform so the forward vector points at target's current position.
     */
    function fromLookAt(position, target, worldUp = Vector3.Up()) {
        const result = Quaternion.Identity();
        fromLookAtToRef(position, target, worldUp, result);
        return result;
    }
    Quaternion.fromLookAt = fromLookAt;
    /**
     * @public
     * Rotates the transform so the forward vector points at target's current position.
     */
    function fromLookAtToRef(position, target, worldUp = Vector3.Up(), result) {
        const m = Matrix.Identity();
        Matrix.lookAtLHToRef(position, target, worldUp, m);
        Matrix.invertToRef(m, m);
        Quaternion.fromRotationMatrixToRef(m, result);
    }
    Quaternion.fromLookAtToRef = fromLookAtToRef;
})(Quaternion || (Quaternion = {}));

/**
 * @public
 * Color4 is a type and a namespace.
 * ```
 * // The namespace contains all types and functions to operates with Color4
 * Color4.add(blue, red) // sum component by component resulting pink
 * // The type Color4 is an alias to Color4.ReadonlyColor4
 * const readonlyBlue: Color4 = Color4.Blue()
 * readonlyBlue.a = 0.1 // this FAILS
 *
 * // For mutable usage, use `Color4.Mutable`
 * const blue: Color4.Mutable = Color4.Blue()
 * blue.a = 0.1 // this WORKS
 * ```
 */
var Color4;
(function (Color4) {
    /**
     * Creates create mutable Color4 from red, green, blue values, all between 0 and 1
     * @param r - defines the red component (between 0 and 1, default is 0)
     * @param g - defines the green component (between 0 and 1, default is 0)
     * @param b - defines the blue component (between 0 and 1, default is 0)
     * @param a - defines the alpha component (between 0 and 1, default is 1)
     */
    function create(
    /**
     * Defines the red component (between 0 and 1, default is 0)
     */
    r = 0, 
    /**
     * Defines the green component (between 0 and 1, default is 0)
     */
    g = 0, 
    /**
     * Defines the blue component (between 0 and 1, default is 0)
     */
    b = 0, 
    /**
     * Defines the alpha component (between 0 and 1, default is 1)
     */
    a = 1) {
        return { r, g, b, a };
    }
    Color4.create = create;
    // Statics
    /**
     * Creates a Color4 from the string containing valid hexadecimal values
     * @param hex - defines a string containing valid hexadecimal values
     * @returns create mutable Color4
     */
    function fromHexString(hex) {
        if (hex.substring(0, 1) !== '#' || hex.length !== 9) {
            return create(0.0, 0.0, 0.0, 0.0);
        }
        const r = parseInt(hex.substring(1, 3), 16);
        const g = parseInt(hex.substring(3, 5), 16);
        const b = parseInt(hex.substring(5, 7), 16);
        const a = parseInt(hex.substring(7, 9), 16);
        return Color4.fromInts(r, g, b, a);
    }
    Color4.fromHexString = fromHexString;
    /**
     * Creates create mutable Color4  set with the linearly interpolated values of "amount" between the left Color4 object and the right Color4 object
     * @param left - defines the start value
     * @param right - defines the end value
     * @param amount - defines the gradient factor
     * @returns create mutable Color4
     */
    function lerp(left, right, amount) {
        const result = create(0.0, 0.0, 0.0, 0.0);
        Color4.lerpToRef(left, right, amount, result);
        return result;
    }
    Color4.lerp = lerp;
    /**
     * Set the given "result" with the linearly interpolated values of "amount" between the left Color4 object and the right Color4 object
     * @param left - defines the start value
     * @param right - defines the end value
     * @param amount - defines the gradient factor
     * @param result - defines the Color4 object where to store data
     */
    function lerpToRef(left, right, amount, result) {
        result.r = left.r + (right.r - left.r) * amount;
        result.g = left.g + (right.g - left.g) * amount;
        result.b = left.b + (right.b - left.b) * amount;
        result.a = left.a + (right.a - left.a) * amount;
    }
    Color4.lerpToRef = lerpToRef;
    /**
     * Returns a Color4 value containing a red color
     * @returns a new Color4
     */
    function Red() {
        return create(1.0, 0, 0, 1.0);
    }
    Color4.Red = Red;
    /**
     * Returns a Color4 value containing a green color
     * @returns create mutable Color4
     */
    function Green() {
        return create(0, 1.0, 0, 1.0);
    }
    Color4.Green = Green;
    /**
     * Returns a Color4 value containing a blue color
     * @returns create mutable Color4
     */
    function Blue() {
        return create(0, 0, 1.0, 1.0);
    }
    Color4.Blue = Blue;
    /**
     * Returns a Color4 value containing a black color
     * @returns create mutable Color4
     */
    function Black() {
        return create(0, 0, 0, 1);
    }
    Color4.Black = Black;
    /**
     * Returns a Color4 value containing a white color
     * @returns create mutable Color4
     */
    function White() {
        return create(1, 1, 1, 1);
    }
    Color4.White = White;
    /**
     * Returns a Color4 value containing a purple color
     * @returns create mutable Color4
     */
    function Purple() {
        return create(0.5, 0, 0.5, 1);
    }
    Color4.Purple = Purple;
    /**
     * Returns a Color4 value containing a magenta color
     * @returns create mutable Color4
     */
    function Magenta() {
        return create(1, 0, 1, 1);
    }
    Color4.Magenta = Magenta;
    /**
     * Returns a Color4 value containing a yellow color
     * @returns create mutable Color4
     */
    function Yellow() {
        return create(1, 1, 0, 1);
    }
    Color4.Yellow = Yellow;
    /**
     * Returns a Color4 value containing a gray color
     * @returns create mutable Color4
     */
    function Gray() {
        return create(0.5, 0.5, 0.5, 1.0);
    }
    Color4.Gray = Gray;
    /**
     * Returns a Color4 value containing a teal color
     * @returns create mutable Color4
     */
    function Teal() {
        return create(0, 1.0, 1.0, 1.0);
    }
    Color4.Teal = Teal;
    /**
     * Returns a Color4 value containing a transparent color
     * @returns create mutable Color4
     */
    function Clear() {
        return create(0, 0, 0, 0);
    }
    Color4.Clear = Clear;
    /**
     * Creates a Color4 from a Color3 and an alpha value
     * @param color3 - defines the source Color3 to read from
     * @param alpha - defines the alpha component (1.0 by default)
     * @returns create mutable Color4
     */
    function fromColor3(color3, alpha = 1.0) {
        return create(color3.r, color3.g, color3.b, alpha);
    }
    Color4.fromColor3 = fromColor3;
    /**
     * Creates a Color4 from the starting index element of the given array
     * @param array - defines the source array to read from
     * @param offset - defines the offset in the source array
     * @returns create mutable Color4
     */
    function fromArray(array, offset = 0) {
        return create(array[offset], array[offset + 1], array[offset + 2], array[offset + 3]);
    }
    Color4.fromArray = fromArray;
    /**
     * Creates a new Color3 from integer values (less than 256)
     * @param r - defines the red component to read from (value between 0 and 255)
     * @param g - defines the green component to read from (value between 0 and 255)
     * @param b - defines the blue component to read from (value between 0 and 255)
     * @param a - defines the alpha component to read from (value between 0 and 255)
     * @returns a new Color4
     */
    function fromInts(r, g, b, a) {
        return create(r / 255.0, g / 255.0, b / 255.0, a / 255.0);
    }
    Color4.fromInts = fromInts;
    /**
     * Check the content of a given array and convert it to an array containing RGBA data
     * If the original array was already containing count * 4 values then it is returned directly
     * @param colors - defines the array to check
     * @param count - defines the number of RGBA data to expect
     * @returns an array containing count * 4 values (RGBA)
     */
    function checkColors4(colors, count) {
        // Check if color3 was used
        if (colors.length === count * 3) {
            const colors4 = [];
            for (let index = 0; index < colors.length; index += 3) {
                const newIndex = (index / 3) * 4;
                colors4[newIndex] = colors[index];
                colors4[newIndex + 1] = colors[index + 1];
                colors4[newIndex + 2] = colors[index + 2];
                colors4[newIndex + 3] = 1.0;
            }
            return colors4;
        }
        return colors;
    }
    Color4.checkColors4 = checkColors4;
    // Operators
    /**
     * Adds  the given Color4 values to the ref Color4 object
     * @param a - defines the first operand
     * @param b - defines the second operand
     * @param ref - defines the result rference
     * @returns
     */
    function addToRef(a, b, ref) {
        ref.r = a.r + b.r;
        ref.g = a.g + b.g;
        ref.b = a.b + b.b;
        ref.a = a.a + b.a;
    }
    Color4.addToRef = addToRef;
    /**
     * Stores from the starting index in the given array the Color4 successive values
     * @param array - defines the array where to store the r,g,b components
     * @param index - defines an optional index in the target array to define where to start storing values
     * @returns the current Color4 object
     */
    function toArray(value, array, index = 0) {
        array[index] = value.r;
        array[index + 1] = value.g;
        array[index + 2] = value.b;
        array[index + 3] = value.a;
    }
    Color4.toArray = toArray;
    /**
     * Creates a Color4 set with the added values of the current Color4 and of the given one
     * @param right - defines the second operand
     * @returns create mutable Color4
     */
    function add(value, right) {
        const ret = Clear();
        addToRef(value, right, ret);
        return ret;
    }
    Color4.add = add;
    /**
     * Creates a Color4 set with the subtracted values of the given one from the current Color4
     * @param right - defines the second operand
     * @returns create mutable Color4
     */
    function subtract(value, right) {
        const ret = Clear();
        subtractToRef(value, right, ret);
        return ret;
    }
    Color4.subtract = subtract;
    /**
     * Subtracts the given ones from the current Color4 values and stores the results in "result"
     * @param right - defines the second operand
     * @param result - defines the Color4 object where to store the result
     * @returns the current Color4 object
     */
    function subtractToRef(a, b, result) {
        result.r = a.r - b.r;
        result.g = a.g - b.g;
        result.b = a.b - b.b;
        result.a = a.a - b.a;
    }
    Color4.subtractToRef = subtractToRef;
    /**
     * Creates a Color4 with the current Color4 values multiplied by scale
     * @param scale - defines the scaling factor to apply
     * @returns create mutable Color4
     */
    function scale(value, scale) {
        return create(value.r * scale, value.g * scale, value.b * scale, value.a * scale);
    }
    Color4.scale = scale;
    /**
     * Multiplies the current Color4 values by scale and stores the result in "result"
     * @param scale - defines the scaling factor to apply
     * @param result - defines the Color4 object where to store the result
     */
    function scaleToRef(value, scale, result) {
        result.r = value.r * scale;
        result.g = value.g * scale;
        result.b = value.b * scale;
        result.a = value.a * scale;
    }
    Color4.scaleToRef = scaleToRef;
    /**
     * Scale the current Color4 values by a factor and add the result to a given Color4
     * @param scale - defines the scale factor
     * @param result - defines the Color4 object where to store the result
     */
    function scaleAndAddToRef(value, scale, result) {
        result.r += value.r * scale;
        result.g += value.g * scale;
        result.b += value.b * scale;
        result.a += value.a * scale;
    }
    Color4.scaleAndAddToRef = scaleAndAddToRef;
    /**
     * Clamps the rgb values by the min and max values and stores the result into "result"
     * @param min - defines minimum clamping value (default is 0)
     * @param max - defines maximum clamping value (default is 1)
     * @param result - defines color to store the result into.
     */
    function clampToRef(value, min = 0, max = 1, result) {
        result.r = Scalar.clamp(value.r, min, max);
        result.g = Scalar.clamp(value.g, min, max);
        result.b = Scalar.clamp(value.b, min, max);
        result.a = Scalar.clamp(value.a, min, max);
    }
    Color4.clampToRef = clampToRef;
    /**
     * Multipy an Color4 value by another and return create mutable Color4
     * @param color - defines the Color4 value to multiply by
     * @returns create mutable Color4
     */
    function multiply(value, color) {
        return create(value.r * color.r, value.g * color.g, value.b * color.b, value.a * color.a);
    }
    Color4.multiply = multiply;
    /**
     * Multipy a Color4 value by another and push the result in a reference value
     * @param color - defines the Color4 value to multiply by
     * @param result - defines the Color4 to fill the result in
     * @returns the result Color4
     */
    function multiplyToRef(value, color, result) {
        result.r = value.r * color.r;
        result.g = value.g * color.g;
        result.b = value.b * color.b;
        result.a = value.a * color.a;
    }
    Color4.multiplyToRef = multiplyToRef;
    /**
     * Creates a string with the Color4 current values
     * @returns the string representation of the Color4 object
     */
    function toString(value) {
        return ('{R: ' +
            value.r +
            ' G:' +
            value.g +
            ' B:' +
            value.b +
            ' A:' +
            value.a +
            '}');
    }
    Color4.toString = toString;
    /**
     * Compute the Color4 hash code
     * @returns an unique number that can be used to hash Color4 objects
     */
    function getHashCode(value) {
        let hash = value.r || 0;
        hash = (hash * 397) ^ (value.g || 0);
        hash = (hash * 397) ^ (value.b || 0);
        hash = (hash * 397) ^ (value.a || 0);
        return hash;
    }
    Color4.getHashCode = getHashCode;
    /**
     * Creates a Color4 copied from the current one
     * @returns create mutable Color4
     */
    function clone(value) {
        return create(value.r, value.g, value.b, value.a);
    }
    Color4.clone = clone;
    /**
     * Copies the given Color4 values into the destination
     * @param source - defines the source Color4 object
     * @param dest - defines the destination Color4 object
     * @returns
     */
    function copyFrom(source, dest) {
        dest.r = source.r;
        dest.g = source.g;
        dest.b = source.b;
        dest.a = source.a;
    }
    Color4.copyFrom = copyFrom;
    /**
     * Copies the given float values into the current one
     * @param r - defines the red component to read from
     * @param g - defines the green component to read from
     * @param b - defines the blue component to read from
     * @param a - defines the alpha component to read from
     * @returns the current updated Color4 object
     */
    function copyFromFloats(r, g, b, a, dest) {
        dest.r = r;
        dest.g = g;
        dest.b = b;
        dest.a = a;
    }
    Color4.copyFromFloats = copyFromFloats;
    /**
     * Copies the given float values into the current one
     * @param r - defines the red component to read from
     * @param g - defines the green component to read from
     * @param b - defines the blue component to read from
     * @param a - defines the alpha component to read from
     * @returns the current updated Color4 object
     */
    function set(r, g, b, a, dest) {
        dest.r = r;
        dest.g = g;
        dest.b = b;
        dest.a = a;
    }
    Color4.set = set;
    /**
     * Compute the Color4 hexadecimal code as a string
     * @returns a string containing the hexadecimal representation of the Color4 object
     */
    function toHexString(value) {
        const intR = (value.r * 255) | 0;
        const intG = (value.g * 255) | 0;
        const intB = (value.b * 255) | 0;
        const intA = (value.a * 255) | 0;
        return ('#' +
            Scalar.toHex(intR) +
            Scalar.toHex(intG) +
            Scalar.toHex(intB) +
            Scalar.toHex(intA));
    }
    Color4.toHexString = toHexString;
    /**
     * Computes a Color4 converted from the current one to linear space
     * @returns create mutable Color4
     */
    function toLinearSpace(value) {
        const convertedColor = create();
        toLinearSpaceToRef(value, convertedColor);
        return convertedColor;
    }
    Color4.toLinearSpace = toLinearSpace;
    /**
     * Converts the Color4 values to linear space and stores the result in "convertedColor"
     * @param convertedColor - defines the Color4 object where to store the linear space version
     * @returns the unmodified Color4
     */
    function toLinearSpaceToRef(value, ref) {
        ref.r = Math.pow(value.r, ToLinearSpace);
        ref.g = Math.pow(value.g, ToLinearSpace);
        ref.b = Math.pow(value.b, ToLinearSpace);
        ref.a = value.a;
    }
    Color4.toLinearSpaceToRef = toLinearSpaceToRef;
    /**
     * Computes a Color4 converted from the current one to gamma space
     * @returns create mutable Color4
     */
    function toGammaSpace(value) {
        const convertedColor = create();
        toGammaSpaceToRef(value, convertedColor);
        return convertedColor;
    }
    Color4.toGammaSpace = toGammaSpace;
    /**
     * Converts the Color4 values to gamma space and stores the result in "convertedColor"
     * @param convertedColor - defines the Color4 object where to store the gamma space version
     * @returns the unmodified Color4
     */
    function toGammaSpaceToRef(value, convertedColor) {
        convertedColor.r = Math.pow(value.r, ToGammaSpace);
        convertedColor.g = Math.pow(value.g, ToGammaSpace);
        convertedColor.b = Math.pow(value.b, ToGammaSpace);
        convertedColor.a = value.a;
    }
    Color4.toGammaSpaceToRef = toGammaSpaceToRef;
})(Color4 || (Color4 = {}));

const Cube = engine.defineComponent('cube-id', {});

function createCube(x, y, z, spawner = true) {
    const meshEntity = engine.addEntity();
    Cube.create(meshEntity);
    Transform.create(meshEntity, { position: { x, y, z } });
    MeshRenderer.setBox(meshEntity);
    MeshCollider.setBox(meshEntity);
    if (spawner) {
        PointerEvents.create(meshEntity, {
            pointerEvents: [
                {
                    eventType: 1,
                    eventInfo: {
                        button: 1,
                        hoverText: 'Press E to spawn',
                        maxDistance: 100,
                        showFeedback: true
                    }
                }
            ]
        });
    }
    return meshEntity;
}

const BounceScaling = engine.defineComponent('BounceScaling', { t: Schemas.Number });
function circularSystem(dt) {
    const entitiesWithMeshRenderer = engine.getEntitiesWith(MeshRenderer, Transform);
    for (const [entity, _meshRenderer, _transform] of entitiesWithMeshRenderer) {
        const mutableTransform = Transform.getMutable(entity);
        mutableTransform.rotation = Quaternion.multiply(mutableTransform.rotation, Quaternion.fromAngleAxis(dt * 10, Vector3.Up()));
    }
}
function spawnerSystem() {
    const clickedCubes = engine.getEntitiesWith(PointerEvents);
    for (const [entity] of clickedCubes) {
        if (inputSystem.isTriggered(1, 1, entity)) {
            createCube(1 + Math.random() * 8, Math.random() * 8, 1 + Math.random() * 8, false);
            BounceScaling.createOrReplace(entity);
        }
    }
}
function bounceScalingSystem(dt) {
    const clickedCubes = engine.getEntitiesWith(BounceScaling, Transform);
    for (const [entity] of clickedCubes) {
        const m = BounceScaling.getMutable(entity);
        m.t += dt;
        if (m.t > 5) {
            Transform.getMutable(entity).scale = Vector3.One();
            BounceScaling.deleteFrom(entity);
        }
        else {
            const factor = 0.9 + 0.2 * Math.exp(-1.5 * m.t) * Math.sin(10 * m.t);
            Transform.getMutable(entity).scale = Vector3.scale(Vector3.One(), factor);
        }
    }
}

class ObserverEventState {
    constructor(mask, skipNextObservers = false, target, currentTarget) {
        this.initalize(mask, skipNextObservers, target, currentTarget);
    }
    initalize(mask, skipNextObservers = false, target, currentTarget) {
        this.mask = mask;
        this.skipNextObservers = skipNextObservers;
        this.target = target;
        this.currentTarget = currentTarget;
        return this;
    }
}
class Observer {
    constructor(callback, mask, scope = null) {
        this.callback = callback;
        this.mask = mask;
        this.scope = scope;
        this.unregisterOnNextCall = false;
        this._willBeUnregistered = false;
    }
}
class Observable {
    constructor(onObserverAdded) {
        this._observers = new Array();
        this._onObserverAdded = null;
        this._eventState = new ObserverEventState(0);
        if (onObserverAdded) {
            this._onObserverAdded = onObserverAdded;
        }
    }
    add(callback, mask = -1, insertFirst = false, scope = null, unregisterOnFirstCall = false) {
        if (!callback) {
            return null;
        }
        const observer = new Observer(callback, mask, scope);
        observer.unregisterOnNextCall = unregisterOnFirstCall;
        if (insertFirst) {
            this._observers.unshift(observer);
        }
        else {
            this._observers.push(observer);
        }
        if (this._onObserverAdded) {
            this._onObserverAdded(observer);
        }
        return observer;
    }
    addOnce(callback) {
        return this.add(callback, undefined, undefined, undefined, true);
    }
    remove(observer) {
        if (!observer) {
            return false;
        }
        const index = this._observers.indexOf(observer);
        if (index !== -1) {
            this._deferUnregister(observer);
            return true;
        }
        return false;
    }
    removeCallback(callback, scope) {
        for (let index = 0; index < this._observers.length; index++) {
            if (this._observers[index].callback === callback && (!scope || scope === this._observers[index].scope)) {
                this._deferUnregister(this._observers[index]);
                return true;
            }
        }
        return false;
    }
    notifyObservers(eventData, mask = -1, target, currentTarget) {
        if (!this._observers.length) {
            return true;
        }
        const state = this._eventState;
        state.mask = mask;
        state.target = target;
        state.currentTarget = currentTarget;
        state.skipNextObservers = false;
        state.lastReturnValue = eventData;
        for (const obs of this._observers) {
            if (obs._willBeUnregistered) {
                continue;
            }
            if (obs.mask & mask) {
                if (obs.scope) {
                    state.lastReturnValue = obs.callback.apply(obs.scope, [eventData, state]);
                }
                else {
                    state.lastReturnValue = obs.callback(eventData, state);
                }
                if (obs.unregisterOnNextCall) {
                    this._deferUnregister(obs);
                }
            }
            if (state.skipNextObservers) {
                return false;
            }
        }
        return true;
    }
    notifyObserversWithPromise(eventData, mask = -1, target, currentTarget) {
        let p = Promise.resolve(eventData);
        if (!this._observers.length) {
            return p;
        }
        const state = this._eventState;
        state.mask = mask;
        state.target = target;
        state.currentTarget = currentTarget;
        state.skipNextObservers = false;
        this._observers.forEach((obs) => {
            if (state.skipNextObservers) {
                return;
            }
            if (obs._willBeUnregistered) {
                return;
            }
            if (obs.mask & mask) {
                if (obs.scope) {
                    p = p.then((lastReturnedValue) => {
                        state.lastReturnValue = lastReturnedValue;
                        return obs.callback.apply(obs.scope, [eventData, state]);
                    });
                }
                else {
                    p = p.then((lastReturnedValue) => {
                        state.lastReturnValue = lastReturnedValue;
                        return obs.callback(eventData, state);
                    });
                }
                if (obs.unregisterOnNextCall) {
                    this._deferUnregister(obs);
                }
            }
        });
        return p.then(() => {
            return eventData;
        });
    }
    notifyObserver(observer, eventData, mask = -1) {
        const state = this._eventState;
        state.mask = mask;
        state.skipNextObservers = false;
        observer.callback(eventData, state);
    }
    hasObservers() {
        return this._observers.length > 0;
    }
    clear() {
        this._observers = new Array();
        this._onObserverAdded = null;
    }
    clone() {
        const result = new Observable();
        result._observers = this._observers.slice(0);
        return result;
    }
    hasSpecificMask(mask = -1) {
        for (const obs of this._observers) {
            if (obs.mask & mask || obs.mask === mask) {
                return true;
            }
        }
        return false;
    }
    _deferUnregister(observer) {
        observer.unregisterOnNextCall = false;
        observer._willBeUnregistered = true;
        Promise.resolve()
            .then.bind(Promise.resolve())(async () => this._remove(observer))
            .catch(console.error);
    }
    _remove(observer) {
        if (!observer) {
            return false;
        }
        const index = this._observers.indexOf(observer);
        if (index !== -1) {
            this._observers.splice(index, 1);
            return true;
        }
        return false;
    }
}

let subscribeFunction = EngineApi.subscribe;
function createSubscriber(eventName) {
    return () => {
        subscribeFunction({ eventId: eventName }).catch(console.error);
    };
}
const onEnterSceneObservable = new Observable(createSubscriber('onEnterScene'));
const onLeaveSceneObservable = new Observable(createSubscriber('onLeaveScene'));
const onSceneReadyObservable = new Observable(createSubscriber('sceneStart'));
const onPlayerExpressionObservable = new Observable(createSubscriber('playerExpression'));
const onVideoEvent = new Observable(createSubscriber('videoEvent'));
const onProfileChanged = new Observable(createSubscriber('profileChanged'));
const onPlayerConnectedObservable = new Observable(createSubscriber('playerConnected'));
const onPlayerDisconnectedObservable = new Observable(createSubscriber('playerDisconnected'));
const onRealmChangedObservable = new Observable(createSubscriber('onRealmChanged'));
const onPlayerClickedObservable = new Observable(createSubscriber('playerClicked'));
const onCommsMessage = new Observable(createSubscriber('comms'));
async function pollEvents(sendBatch) {
    const { events } = await sendBatch({ actions: [] });
    for (const e of events) {
        if (e.generic) {
            const data = JSON.parse(e.generic.eventData);
            switch (e.generic.eventId) {
                case 'onEnterScene': {
                    onEnterSceneObservable.notifyObservers(data);
                    break;
                }
                case 'onLeaveScene': {
                    onLeaveSceneObservable.notifyObservers(data);
                    break;
                }
                case 'sceneStart': {
                    onSceneReadyObservable.notifyObservers(data);
                    break;
                }
                case 'playerExpression': {
                    onPlayerExpressionObservable.notifyObservers(data);
                    break;
                }
                case 'videoEvent': {
                    const videoData = data;
                    onVideoEvent.notifyObservers(videoData);
                    break;
                }
                case 'profileChanged': {
                    onProfileChanged.notifyObservers(data);
                    break;
                }
                case 'playerConnected': {
                    onPlayerConnectedObservable.notifyObservers(data);
                    break;
                }
                case 'playerDisconnected': {
                    onPlayerDisconnectedObservable.notifyObservers(data);
                    break;
                }
                case 'onRealmChanged': {
                    onRealmChangedObservable.notifyObservers(data);
                    break;
                }
                case 'playerClicked': {
                    onPlayerClickedObservable.notifyObservers(data);
                    break;
                }
                case 'comms': {
                    onCommsMessage.notifyObservers(data);
                    break;
                }
            }
        }
    }
}

function createRendererTransport(engineApi) {
    async function sendToRenderer(message) {
        const response = await engineApi.crdtSendToRenderer({
            data: new Uint8Array(message)
        });
        if (response && response.data && response.data.length) {
            if (rendererTransport.onmessage) {
                for (const byteArray of response.data) {
                    rendererTransport.onmessage(byteArray);
                }
            }
        }
    }
    const rendererTransport = {
        async send(message) {
            try {
                await sendToRenderer(message);
            }
            catch (error) {
                console.error(error);
                debugger;
            }
        },
        filter(message) {
            if (message.componentId > MAX_STATIC_COMPONENT) {
                return false;
            }
            return !!message;
        }
    };
    return rendererTransport;
}

const rendererTransport = createRendererTransport({ crdtSendToRenderer: EngineApi.crdtSendToRenderer });
engine.addTransport(rendererTransport);
async function onUpdate(deltaTime) {
    await engine.update(deltaTime);
    await pollEvents(EngineApi.sendBatch);
}
async function onStart() {
    await engine.seal();
    const response = await EngineApi.crdtGetState({ data: new Uint8Array() });
    if (!!rendererTransport.onmessage) {
        if (response && response.data && response.data.length) {
            for (const byteArray of response.data) {
                rendererTransport.onmessage(byteArray);
            }
        }
    }
}

engine.addSystem(circularSystem);
engine.addSystem(spawnerSystem);
engine.addSystem(bounceScalingSystem);
executeTask(async function () {
    const cube = createCube(-2, 1, -2);
    Material.setPbrMaterial(cube, { albedoColor: Color4.create(1.0, 0.0, 0.42) });
    for (let x = 0.5; x < 16; x += 1) {
        for (let y = 0.5; y < 16; y += 1) {
            createCube(x, 0, y, false);
        }
    }
});
let hoverState = 0;
engine.addSystem(function CircleHoverSystem(dt) {
    hoverState += Math.PI * dt * 0.5;
    const entitiesWithBoxShapes = engine.getEntitiesWith(MeshRenderer, Transform);
    for (const [entity] of entitiesWithBoxShapes) {
        const transform = Transform.getMutable(entity);
        transform.position.y =
            Math.cos(hoverState + Math.sqrt(Math.pow(transform.position.x - 8, 2) + Math.pow(transform.position.z - 8, 2)) / Math.PI) *
                2 +
                2;
    }
});

exports.onStart = onStart;
exports.onUpdate = onUpdate;
