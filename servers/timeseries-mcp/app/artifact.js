// @ts-check
import {parse, stringify, validate, version} from "uuid";

/** Artifact's qualified ID aliases share one UUIDv7 occurrence identity. @param {string} wire */
export function artifactIdentity(wire){
 let canonical=wire;
 if(/^urn:uuid:/u.test(wire))canonical=wire.substring(9);
 else if(/^\{[^{}]+\}$/u.test(wire))canonical=wire.substring(1,wire.length-1);
 else if(/^[0-9a-fA-F]{32}$/u.test(wire))canonical=`${wire.substring(0,8)}-${wire.substring(8,12)}-${wire.substring(12,16)}-${wire.substring(16,20)}-${wire.substring(20)}`;
 if(!validate(canonical)||version(canonical)!==7)throw new Error("Invalid Artifact occurrence");
 return stringify(parse(canonical));
}

/** Parse the Artifact owner's neutral or presented resource profile. @param {string} wire */
export function artifactAddress(wire){
 const uri=new URL(wire);
 if(uri.href!==wire||!(/^[a-z][a-z0-9+.-]*:$/u.test(uri.protocol))||uri.username||uri.password||uri.port||wire.includes("%")||wire.includes("?")||wire.includes("#")||wire.includes("{")||wire.includes("}"))throw new Error("Invalid Artifact address components");
 if(uri.protocol==="artifact:"&&uri.pathname==="")return artifactIdentity(uri.hostname);
 const path=/^\/([^/]+)$/u.exec(uri.pathname);
 if(uri.hostname!=="artifact"||!path)throw new Error("Invalid Artifact address route");
 return artifactIdentity(path[1]);
}

/** Timeseries emits and admits only its canonical presentation. @param {string} wire */
export function forecastArtifactIdentity(wire){
 const identity=artifactAddress(wire);
 if(new URL(identity,"timeseries://artifact/").href!==wire)throw new Error("Invalid Timeseries result address");
 return identity;
}
