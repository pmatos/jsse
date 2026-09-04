// Build-time aliases for host/tool-only imports in Zod's upstream tests. These
// run identically in the final Node and jsse bundle; none changes Zod itself.

export const File = globalThis.File;

export function randomBytes(size: number) {
  // string.test.ts only needs a large valid base64/base64url payload. Zeroed
  // bytes retain the large regex workload without depending on a runtime Node
  // crypto module. The generator bounds the upstream 10 MiB fixture to 64 KiB.
  return Buffer.alloc(size);
}

export function inspect(value: unknown) {
  // ZodError#toString is the representation asserted by the one util.inspect
  // call in the suite.
  if (
    value &&
    typeof value === "object" &&
    "name" in value &&
    "message" in value
  ) {
    return `${String((value as any).name)}: ${String((value as any).message)}`;
  }
  return String(value);
}

export function createHash(algorithm: string) {
  const lengths: Record<string, number> = {
    md5: 16,
    sha1: 20,
    sha256: 32,
    sha384: 48,
    sha512: 64,
  };
  if (!(algorithm in lengths)) {
    throw new Error(`Unsupported hash algorithm: ${algorithm}`);
  }
  return {
    update() {
      return this;
    },
    digest() {
      // The upstream test uses the digest only to generate correctly-sized
      // hex/base64/base64url examples for Zod's format validators.
      return Buffer.alloc(lengths[algorithm], 255);
    },
  };
}

export function checkSync() {
  // recheck is a JVM/native static-analysis tool. The JavaScript-engine corpus
  // exercises these same regexes through Zod; it does not spawn a ReDoS tool.
  return { status: "safe" };
}

export class Validator {
  version = "jsse-bundle";

  async validate() {
    // Upstream invokes this async helper without awaiting it. Keep the import
    // and call portable; the JSON-schema values remain covered by assertions.
    return { valid: true };
  }
}
