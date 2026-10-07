"""Current-key admission shared by controlled SDK wire objects."""
from pydantic import BaseModel, model_validator


class CurrentWireModel(BaseModel):
    @model_validator(mode="before")
    @classmethod
    def current_wire_keys(cls, value: object) -> object:
        if isinstance(value, dict):
            admitted = {field.alias or name for name, field in cls.model_fields.items()}
            if any(key not in admitted for key in value):
                raise ValueError("wire object contains an unsupported field")
        return value
