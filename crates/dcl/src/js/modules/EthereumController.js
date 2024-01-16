const allowMethodlist = [
    'eth_sendTransaction',
    'eth_getTransactionReceipt',
    'eth_estimateGas',
    'eth_call',
    'eth_getBalance',
    'eth_getStorageAt',
    'eth_blockNumber',
    'eth_gasPrice',
    'eth_protocolVersion',
    'net_version',
    'web3_sha3',
    'web3_clientVersion',
    'eth_getTransactionCount',
    'eth_getBlockByNumber',
    'eth_requestAccounts',
    'eth_signTypedData_v4',
    'eth_getCode'
]

module.exports.sendAsync = async function (message) {
    if (
        typeof message !== 'object' ||
        typeof message.id !== 'number' ||
        typeof message.method !== 'string' ||
        typeof message.jsonParams !== 'string'
    ) {
        throw new Error('Invalid JSON-RPC message')
    }

    if (!allowMethodlist.includes(message.method)) {
        throw new Error(`The Ethereum method "${message.method}" is not allowed on Decentraland Provider`)
    }

    const resValue = await Deno.core.ops.op_send_async(message.method, message.jsonParams)

    const result = {
        id: message.id,
        jsonrpc: "2.0",
        result: resValue
    }

    return {
        jsonAnyResponse: JSON.stringify(result)
    }
}

module.exports.requirePayment = async function (body) {
    throw new Error("`requirePayment is not implemented, this method is deprecated in SDK7 APIs, please use sendAsync instead, you can use a library like ethers.js.")
}
module.exports.signMessage = async function (body) {
    throw new Error("signMessage is not implemented, this method is deprecated in SDK7 APIs, please use sendAsync instead, you can use a library like ethers.js.")
}
module.exports.convertMessageToObject = async function (body) {
    throw new Error("convertMessageToObject is not implemented, this method is deprecated in SDK7 APIs, please use sendAsync instead, you can use a library like ethers.js.")
}
module.exports.getUserAccount = async function (body) {
    throw new Error("getUserAccount is not implemented, this method is deprecated in SDK7 APIs, please use sendAsync instead, you can use a library like ethers.js.")
}
