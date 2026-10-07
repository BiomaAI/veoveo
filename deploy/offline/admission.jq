# One current admission profile shared by the connected builder and offline loader.
def closed($fields): type == "object" and (keys | sort) == ($fields | sort);
def text: type == "string" and length > 0;
def tagged: text and (test("\\s") | not) and test(":[^:/@]+\\z") and (endswith(":latest") | not);
def external:
  closed(["ref", "source"]) and (.ref | tagged)
  and (.source | text and (test("\\s") | not) and test("@sha256:[a-f0-9]{64}\\z"));
def owned:
  closed(["ref", "dockerfile"]) and (.ref | tagged) and (.dockerfile | text);
def selected: [.externalImages[].ref, .veoveoImages[].ref];
def lock:
  closed(["schemaVersion", "bundleVersion", "externalImages", "veoveoImages"])
  and .schemaVersion == 2 and (.bundleVersion | text)
  and (.externalImages | type == "array" and all(.[]; external))
  and (.veoveoImages | type == "array" and all(.[]; owned))
  and (selected | length > 0 and length == (unique | length));
def image:
  closed(["ref", "imageId"]) and (.ref | tagged)
  and (.imageId | type == "string" and test("^sha256:[a-f0-9]{64}\\z"));
def bundle:
  closed(["schemaVersion", "platform", "bundle", "images"])
  and .schemaVersion == 2
  and (.platform == "linux/amd64" or .platform == "linux/arm64")
  and (.bundle | lock)
  and (.images | type == "array" and all(.[]; image))
  and ([.images[].ref] | length == (unique | length))
  and (([.images[].ref] | sort) == (.bundle | selected | sort));
# Receivers use --slurp: admission covers the complete input stream.
try (type == "array" and length == 1 and
     (.[0] | if $profile == "lock" then lock
             elif $profile == "bundle" then bundle
             else false end)) catch false
