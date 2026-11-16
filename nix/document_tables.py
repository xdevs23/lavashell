# SPDX-License-Identifier: GPL-3.0-only

import enum
import json
from collections.abc import Mapping
from dataclasses import dataclass

from verification_errors import Cause, NextStep, VerificationError

type DocumentValue = str | int | float | bool | None | tuple[DocumentValue, ...] | Mapping[str, DocumentValue]
type TomlValue = str | int | float | bool | list[TomlValue] | dict[str, TomlValue]
type TomlTable = dict[str, TomlValue]


class ValueKind(enum.Enum):
    OBJECT = enum.auto()
    ARRAY = enum.auto()
    STRING = enum.auto()
    INTEGER = enum.auto()
    VALUE = enum.auto()

    @property
    def text(self) -> str:
        match self:
            case ValueKind.OBJECT:
                return "an object"
            case ValueKind.ARRAY:
                return "an array"
            case ValueKind.STRING:
                return "a string"
            case ValueKind.INTEGER:
                return "an integer"
            case ValueKind.VALUE:
                return "a value of a JSON or TOML document"


@dataclass(frozen=True)
class MalformedDocument(Cause):
    location: str
    expected: ValueKind
    next_step = NextStep.REPORT_DEFECT

    def message(self) -> str:
        return f"The field {self.location} is not {self.expected.text}."


def frozen_value(value: object, location: str) -> DocumentValue:
    match value:
        case str() | bool() | int() | float() | None:
            return value
        case list():
            return tuple(frozen_value(item, f"{location}[{index}]") for index, item in enumerate(value))
        case dict():
            return {str(key): frozen_value(item, f"{location}.{key}") for key, item in value.items()}
    raise VerificationError(MalformedDocument(location=location, expected=ValueKind.VALUE))


@dataclass(frozen=True)
class DocumentTable:
    fields: Mapping[str, DocumentValue]
    location: str

    @classmethod
    def of(cls, value: DocumentValue, location: str) -> "DocumentTable":
        if not isinstance(value, Mapping):
            raise VerificationError(MalformedDocument(location=location, expected=ValueKind.OBJECT))
        return cls(value, location)

    @classmethod
    def decoded(cls, value: object, location: str) -> "DocumentTable":
        return cls.of(frozen_value(value, location), location)

    @classmethod
    def parse(cls, text: str, location: str) -> "DocumentTable":
        return cls.decoded(json.loads(text), location)

    def has(self, key: str) -> bool:
        return key in self.fields

    def keys(self) -> tuple[str, ...]:
        return tuple(self.fields)

    def string(self, key: str) -> str:
        return self.checked(key, str, ValueKind.STRING)

    def optional_string(self, key: str) -> str | None:
        return None if self.fields.get(key) is None else self.string(key)

    def integer(self, key: str) -> int:
        value = self.fields.get(key)
        if isinstance(value, bool) or not isinstance(value, int):
            raise self.malformed(key, ValueKind.INTEGER)
        return value

    def value(self, key: str) -> DocumentValue:
        if key not in self.fields:
            raise self.malformed(key, ValueKind.VALUE)
        return self.fields[key]

    def object(self, key: str) -> "DocumentTable":
        return DocumentTable.of(self.fields.get(key), f"{self.location}.{key}")

    def objects(self, key: str) -> tuple["DocumentTable", ...]:
        return tuple(DocumentTable.of(item, f"{self.location}.{key}[{index}]")
                     for index, item in enumerate(self.array(key)))

    def strings(self, key: str) -> tuple[str, ...]:
        return tuple(self.item(key, index, item, str, ValueKind.STRING) for index, item in enumerate(self.array(key)))

    def array(self, key: str) -> tuple[DocumentValue, ...]:
        return self.checked(key, tuple, ValueKind.ARRAY)

    def checked[T](self, key: str, kind: type[T], expected: ValueKind) -> T:
        value = self.fields.get(key)
        if not isinstance(value, kind):
            raise self.malformed(key, expected)
        return value

    def item[T](self, key: str, index: int, value: DocumentValue, kind: type[T], expected: ValueKind) -> T:
        if isinstance(value, bool) and kind is not bool or not isinstance(value, kind):
            raise VerificationError(MalformedDocument(location=f"{self.location}.{key}[{index}]",
                                                      expected=expected))
        return value

    def malformed(self, key: str, expected: ValueKind) -> VerificationError:
        return VerificationError(MalformedDocument(location=f"{self.location}.{key}", expected=expected))
