// ParseJSON (§25.5.1.1) requires invalid JSON text to throw a SyntaxError but
// does not prescribe its message. These assertions pin the Node-compatible
// token/source context used by the Zod JSON codec.

function assertParseMessage(source, reviver, expected) {
  var error;
  try {
    JSON.parse(source, reviver);
  } catch (caught) {
    error = caught;
  }

  if (!(error instanceof SyntaxError)) {
    throw new Error("expected SyntaxError for " + source);
  }
  if (error.message !== expected) {
    throw new Error(
      "unexpected JSON.parse diagnostic: expected " + expected +
      ", got " + error.message
    );
  }
}

var zodSource = '{"invalid":,}';
var zodMessage = 'Unexpected token \',\', "{"invalid":,}" is not valid JSON';

assertParseMessage(zodSource, undefined, zodMessage);
assertParseMessage(zodSource, function (_key, value) { return value; }, zodMessage);

assertParseMessage(
  '{"outer":{"invalid":,}}',
  undefined,
  'Unexpected token \',\', "{"outer":{"... is not valid JSON'
);

assertParseMessage(
  '~~invalid~~',
  undefined,
  'Unexpected token \'~\', "~~invalid~~" is not valid JSON'
);

assertParseMessage(
  'x'.repeat(20),
  undefined,
  'Unexpected token \'x\', "xxxxxxxxxxxxxxxxxxxx" is not valid JSON'
);

var truncatedMessage =
  'Unexpected token \'x\', "xxxxxxxxxx"... is not valid JSON';
assertParseMessage('x'.repeat(21), undefined, truncatedMessage);
assertParseMessage('x'.repeat(1000000), undefined, truncatedMessage);

assertParseMessage(
  'é'.repeat(21),
  undefined,
  'Unexpected token \'é\', "éééééééééé"... is not valid JSON'
);
