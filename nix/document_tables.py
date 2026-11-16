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
    BOOLEAN = enum.auto()
    VALUE = enum.auto()
    DEPENDENCY_TABLE = enum.auto()

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
            case ValueKind.BOOLEAN:
                return "a boolean"
            case ValueKind.VALUE:
                return "a value of a JSON or TOML document"
            case ValueKind.DEPENDENCY_TABLE:
                return "a dependency table of a Cargo manifest"


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


def toml_value(value: DocumentValue, location: str) -> TomlValue:
    match value:
        case str() | bool() | int() | float():
            return value
        case tuple():
            return [toml_value(item, f"{location}[{index}]") for index, item in enumerate(value)]
        case Mapping():
            return {key: toml_value(item, f"{location}.{key}") for key, item in value.items()}
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

    def scalar(self, key: str) -> str | int | None:
        if key in self.fields:
            match value := self.fields[key]:
                case bool():
                    pass
                case str() | int() | None:
                    return value
        raise self.malformed(key, ValueKind.VALUE)

    def string_unless_false(self, key: str) -> str | None:
        return None if self.fields.get(key) is False else self.string(key)

    def boolean(self, key: str) -> bool:
        return self.checked(key, bool, ValueKind.BOOLEAN)

    def flag(self, key: str) -> bool:
        return self.has(key) and self.boolean(key)

    def value(self, key: str) -> DocumentValue:
        if key not in self.fields:
            raise self.malformed(key, ValueKind.VALUE)
        return self.fields[key]

    def toml_table(self) -> TomlTable:
        return {key: toml_value(value, f"{self.location}.{key}") for key, value in self.fields.items()}

    def object(self, key: str) -> "DocumentTable":
        return DocumentTable.of(self.fields.get(key), f"{self.location}.{key}")

    def optional_object(self, key: str) -> "DocumentTable | None":
        return self.object(key) if self.has(key) else None

    def objects(self, key: str) -> tuple["DocumentTable", ...]:
        return tuple(DocumentTable.of(item, f"{self.location}.{key}[{index}]")
                     for index, item in enumerate(self.array(key)))

    def strings(self, key: str) -> tuple[str, ...]:
        return tuple(self.item(key, index, item, str, ValueKind.STRING) for index, item in enumerate(self.array(key)))

    def string_map(self, key: str) -> dict[str, str]:
        table = self.object(key)
        return {name: table.string(name) for name in table.keys()}

    def integers(self, key: str) -> tuple[int, ...]:
        return tuple(self.item(key, index, item, int, ValueKind.INTEGER) for index, item in enumerate(self.array(key)))

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


def table_at(table: TomlTable, *keys: str) -> TomlTable:
    for key in keys:
        value = table[key]
        if not isinstance(value, dict):
            raise TypeError(f"{key} is not a table")
        table = value
    return table


def array_at(table: TomlTable, *keys: str) -> list[TomlValue]:
    *tables, key = keys
    value = table_at(table, *tables)[key]
    if not isinstance(value, list):
        raise TypeError(f"{key} is not an array")
    return value


def string_at(table: TomlTable, *keys: str) -> str:
    *tables, key = keys
    value = table_at(table, *tables)[key]
    if not isinstance(value, str):
        raise TypeError(f"{key} is not a string")
    return value
