DEFINE NAMESPACE bootstrap_rotation;
USE NS bootstrap_rotation;
DEFINE DATABASE retained;
USE DB retained;
DEFINE TABLE credential_rotation_marker SCHEMAFULL PERMISSIONS NONE;
DEFINE FIELD value ON credential_rotation_marker TYPE string;
CREATE credential_rotation_marker:retained CONTENT { value: $value };
