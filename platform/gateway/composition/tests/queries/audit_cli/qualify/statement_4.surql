DEFINE FIELD OVERWRITE block ON audit_block TYPE object FLEXIBLE;
            UPDATE $block SET block.signature = $signature;
