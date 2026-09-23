import registry from "../../schemas/full-protocol.json" with { type: "json" };

// Full protocols are versioned. A recorded report stays valid under the protocol
// it was measured with; a closed protocol accepts no report generated after it
// closed, so a new run cannot pass as historical by omitting a framework.
export const protocols = registry.protocols;
export const current = protocols.find(({ id }) => id === registry.current);
if (!current?.frameworks.length) throw Error("Invalid full-protocol registry");
// Display order: the current protocol lists every framework ever measured.
export const frameworks = current.frameworks;

export function protocolOf(report) {
  if (report.schema === 1 && report.protocol === undefined)
    return byId("six-framework-v1");
  if (typeof report.protocol !== "string")
    throw Error("Measurement report does not declare its protocol");
  const protocol = byId(report.protocol);
  if (protocol.reportSchema !== report.schema)
    throw Error(
      `Protocol ${protocol.id} requires report schema ${protocol.reportSchema}`,
    );
  return protocol;
}

function byId(id) {
  const protocol = protocols.find((candidate) => candidate.id === id);
  if (!protocol) throw Error(`Unknown benchmark protocol: ${id}`);
  return protocol;
}

export function acceptsFull(protocol, report) {
  if (
    protocol.acceptedBefore &&
    !(Date.parse(report.generatedAt) < Date.parse(protocol.acceptedBefore))
  )
    throw Error(
      `Protocol ${protocol.id} closed at ${protocol.acceptedBefore}; new full reports must use ${current.id} (${current.frameworks.length} frameworks)`,
    );
}
