module.exports.signedFetch = async function (body) { 
    const headers = await Deno.core.ops.op_signed_fetch_headers(body.url, body.init?.method);

    if (!body.init) {
        body.init = { headers: {} };
    }

    if (!body.init.hasOwnProperty("headers")) {
        body.init.headers = {};
    }

    for (var i=0; i< headers.length; i++) {
        body.init.headers[headers[i][0]] = headers[i][1];
    }

    let response = await fetch(body.url, body.init);
    let text = await response.text();

    return {
        ok: response.ok,
        status: response.status,
        statusText: response.statusText,
        headers: response.headers,
        body: text,
    };
}

module.exports.getHeaders = async function (body) { 
    const result = await Deno.core.ops.op_signed_fetch_headers(body.url, body.init?.method)

    const headers = {}
    for (var i=0; i< result.length; i++) {
        headers[result[i][0]] = result[i][1];
    }
    return {
        headers
    }
}
