/** @typedef {import("./generated/uav-sim").AppContracts["connection"]} LiveViewConnection */
/** @typedef {import("./generated/uav-sim").AppContracts["closed"]} CloseLiveViewResult */

/** Own every acknowledged authorization, including acquisitions finishing after teardown. */
export class LiveViewOwnership {
  /** @param {(connection: LiveViewConnection) => Promise<CloseLiveViewResult>} release */
  constructor(release) {
    this.release = release;
    /** @type {Set<Promise<LiveViewConnection>>} */
    this.pending = new Set();
    /** @type {Map<string, number>} */
    this.pendingCameras = new Map();
    /** @type {Map<string, LiveViewConnection>} */
    this.authorizations = new Map();
    /** @type {Map<string, string>} */
    this.closedAuthorizations = new Map();
    /** @type {Map<string, Promise<void>>} */
    this.releasing = new Map();
    /** @type {Set<{cause: unknown, cameraIds: readonly string[], renewalId?: string, renewalUri?: string, fenced: boolean}>} */
    this.unresolvedAcquisitions = new Set();
    this.closing = false;
  }

  /** @param {string} cameraId */
  canAcquire(cameraId) {
    return !this.closing && !this.pendingCameras.has(cameraId) && ![...this.unresolvedAcquisitions].some(value => !value.fenced && value.cameraIds.includes(cameraId));
  }

  /** @param {() => Promise<LiveViewConnection>} acquire @param {(cause: unknown) => boolean} [provesRefusal]
   * @param {{cameraIds: readonly string[], renewal?: LiveViewConnection}} [scope] */
  acquire(acquire, provesRefusal = () => false, scope = {cameraIds: []}) {
    if (this.closing) return Promise.reject(new Error("App is closing"));
    if (scope.cameraIds.some(camera => !this.canAcquire(camera))) return Promise.reject(new Error("Earlier camera acquisition remains unresolved"));
    const cameraIds=[...scope.cameraIds], renewalId=scope.renewal?.stream.liveViewId, renewalUri=scope.renewal?.stream.resourceUri;
    for (const camera of cameraIds) this.pendingCameras.set(camera,(this.pendingCameras.get(camera)??0)+1);
    const finished=()=>{this.pending.delete(operation);for(const camera of cameraIds){const count=this.pendingCameras.get(camera)-1;if(count)this.pendingCameras.set(camera,count);else this.pendingCameras.delete(camera);}};
    // Register before dispatch. A pending request never proves non-acquisition.
    const operation = Promise.resolve().then(acquire).then(connection => {
      if (this.closedAuthorizations.get(connection.stream.liveViewId) !== connection.stream.resourceUri) {
        this.authorizations.set(connection.stream.liveViewId, connection);
      }
      return connection;
    });
    this.pending.add(operation);
    operation.then(finished, cause => {
      // The host may have lost or refused admission of an already-acquired result.
      // Only a schema-admitted, fully correlated owner proof can settle this attempt.
      let settled=false;
      try { settled=provesRefusal(cause); } catch {}
      if (!settled) this.unresolvedAcquisitions.add({cause, cameraIds, renewalId, renewalUri, fenced:renewalId !== undefined && this.closedAuthorizations.get(renewalId) === renewalUri});
      finished();
    });
    return operation;
  }

  /** @param {string} liveViewId */
  close(liveViewId) {
    const connection = this.authorizations.get(liveViewId);
    if (!connection) return Promise.resolve();
    const pending = this.releasing.get(liveViewId);
    if (pending) return pending;
    const operation = Promise.resolve().then(() => this.release(connection)).then(result => {
      if (result.closed !== true || result.resourceUri !== connection.stream.resourceUri) {
        throw new Error("Live-view close response does not settle its owned authorization");
      }
      this.closedAuthorizations.set(liveViewId, connection.stream.resourceUri);
      this.authorizations.delete(liveViewId);
      for (const failure of this.unresolvedAcquisitions) {
        if (failure.renewalId === liveViewId && failure.renewalUri === connection.stream.resourceUri) failure.fenced=true;
      }
    });
    this.releasing.set(liveViewId, operation);
    operation.then(() => this.releasing.delete(liveViewId), () => this.releasing.delete(liveViewId));
    return operation;
  }

  async teardown() {
    this.closing = true;
    // Inert acquisition registration prevents a new request racing this snapshot.
    await Promise.allSettled([...this.pending]);
    const results = await Promise.allSettled([...this.authorizations.keys()].map(id => this.close(id)));
    const failures = results.filter(result => result.status === "rejected");
    if (failures.length) throw new Error("Live-view authorization release failed; retry teardown");
    if ([...this.unresolvedAcquisitions].some(value => !value.fenced)) throw new Error("Live-view acquisition outcome remains unresolved");
    if (this.pending.size || this.authorizations.size) throw new Error("Live-view ownership remains unresolved");
  }
}

/** Standard Apps lifecycle reply; the message pump remains active during retirement. */
export function replyToTeardown(id, post, retire, failed) {
  return Promise.resolve().then(retire).then(() => {
    if (id !== undefined) post({jsonrpc: "2.0", id, result: {}});
  }, cause => {
    failed(cause);
    if (id !== undefined) post({jsonrpc: "2.0", id, error: {
      code: -32603, message: "Live-view teardown remains unresolved; retry teardown",
    }});
  });
}
