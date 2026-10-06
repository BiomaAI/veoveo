import test from "node:test";
import assert from "node:assert/strict";
import {admit,resourceValue,toolValue} from "./contracts.js";
test("Stream collection admission preserves empty/default pages and rejects nested corrupt sessions",()=>{
 const good={sessions:[],limit:100};
 assert.deepEqual(resourceValue("stream://sessions",good),good);
 assert.throws(()=>resourceValue("stream://sessions",{...good,sessions:[{session_id:42}]}));
 assert.throws(()=>resourceValue("stream://sessions",{...good,next_cursor:42}));
 assert.throws(()=>resourceValue("stream://sessions",{...good,extra:true}));
 assert.throws(()=>resourceValue("map://sessions",good));
});
