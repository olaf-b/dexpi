//! The metamodel: what a DEXPI model file declares.

use indexmap::IndexMap;

/// What kind of thing a declaration is.
///
/// These correspond one-to-one with the element names the DEXPI XML schema allows
/// inside a `Model`, so the loader can stay a direct transcription of the file rather
/// than an interpretation of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    Package,
    ConcreteClass,
    AbstractClass,
    Enumeration,
    EnumerationLiteral,
    AggregatedDataType,
    AbstractDataType,
    /// `StringType`, `DoubleType`, `IntegerType`, `BooleanType`, `DateTimeType`,
    /// `UndefinedType` — the `Builtin` model's primitives.
    PrimitiveType,
    DataTypeParameter,
    /// A property whose values are literal data: `<Data property="…">`.
    DataProperty,
    /// A property that owns child objects: `<Components property="…">`.
    CompositionProperty,
    /// A property that points at objects elsewhere: `<References property="…">`.
    ReferenceProperty,
}

impl ItemKind {
    pub(super) fn from_tag(tag: &str) -> Option<Self> {
        Some(match tag {
            "Package" => Self::Package,
            "ConcreteClass" => Self::ConcreteClass,
            "AbstractClass" => Self::AbstractClass,
            "Enumeration" => Self::Enumeration,
            "EnumerationLiteral" => Self::EnumerationLiteral,
            "AggregatedDataType" => Self::AggregatedDataType,
            "AbstractDataType" => Self::AbstractDataType,
            "DataTypeParameter" => Self::DataTypeParameter,
            "DataProperty" => Self::DataProperty,
            "CompositionProperty" => Self::CompositionProperty,
            "ReferenceProperty" => Self::ReferenceProperty,
            "StringType" | "DoubleType" | "IntegerType" | "BooleanType" | "DateTimeType"
            | "UndefinedType" => Self::PrimitiveType,
            _ => return None,
        })
    }

    /// Whether objects of this kind can be instantiated by `<Object type="…">`.
    pub fn is_instantiable(self) -> bool {
        self == Self::ConcreteClass
    }

    pub fn describe(self) -> &'static str {
        match self {
            Self::Package => "package",
            Self::ConcreteClass => "concrete class",
            Self::AbstractClass => "abstract class",
            Self::Enumeration => "enumeration",
            Self::EnumerationLiteral => "enumeration literal",
            Self::AggregatedDataType => "aggregated data type",
            Self::AbstractDataType => "abstract data type",
            Self::PrimitiveType => "primitive type",
            Self::DataTypeParameter => "data type parameter",
            Self::DataProperty => "data property",
            Self::CompositionProperty => "composition property",
            Self::ReferenceProperty => "reference property",
        }
    }
}

/// A named declaration in a model, and anything declared inside it.
///
/// Packages, classes, properties and enumeration literals are all represented the same
/// way because references address them the same way: a dotted path walked through
/// named children. `Core/PhysicalQuantities.PhysicalQuantity.UnitType` descends a
/// package, then a class, then a type parameter without changing rules along the way.
#[derive(Debug, Clone)]
pub struct Item {
    pub kind: ItemKind,
    pub name: String,
    /// The model this was declared in, e.g. `Process`.
    pub model: String,
    /// Dotted path within the model, e.g. `Process.Pumping`, used in diagnostics.
    pub path: String,
    /// Raw `superTypes` references, unresolved. Resolution needs the whole model set.
    pub super_types: Vec<String>,
    /// Every type reference in the declaration's type expression, in source order.
    ///
    /// A property's type can be layered — a `QualifiedValue` bound to a
    /// `PhysicalQuantity` bound in turn to a unit enumeration — and which layer matters
    /// depends on the question. The whole list is kept so callers can pick.
    pub types: Vec<String>,
    /// Strings attached to the declaration under the `MetaData` model, such as
    /// `description` and `un_symbol`.
    ///
    /// `MetaData` is imported by every model but was never published (see
    /// spec/VERSIONS.md), so these are carried as plain text rather than resolved.
    pub metadata: IndexMap<String, String>,
    /// The type a property's values take, as a reference, e.g.
    /// `Process/Enumerations.CompressionMethod`.
    ///
    /// Type expressions in DEXPI can be elaborate — unions, classes bound to type
    /// parameters — and this records only the first concrete reference in one, which is
    /// enough to answer "what kind of value does this hold" without modelling the whole
    /// expression. Full type checking is a larger job.
    pub data_type: Option<String>,
    /// Whether the declared type includes `Builtin/Undefined`, meaning the property may
    /// be present but explicitly unspecified.
    pub admits_undefined: bool,
    /// Lower multiplicity bound; `None` where the declaration omits it.
    pub lower: Option<u32>,
    /// Upper multiplicity bound; `None` means unbounded.
    pub upper: Option<u32>,
    pub children: IndexMap<String, Item>,
}

impl Item {
    /// A reference that names this item, e.g. `Process/Process.Pumping`.
    pub fn qualified_name(&self) -> String {
        format!("{}/{}", self.model, self.path)
    }

    /// A metadata string, e.g. `description` or `un_symbol`.
    pub fn meta(&self, key: &str) -> Option<&str> {
        self.metadata.get(key).map(String::as_str)
    }

    /// The unit enumeration a physical quantity is measured in, if this is one.
    ///
    /// The innermost type reference: a `QualifiedValue` holding a `PhysicalQuantity`
    /// ends its type expression with the unit enumeration that quantity is bound to.
    pub fn unit_type(&self) -> Option<&str> {
        self.types
            .iter()
            .rev()
            .find(|reference| reference.ends_with("Unit"))
            .map(String::as_str)
    }

    pub fn child(&self, name: &str) -> Option<&Item> {
        self.children.get(name)
    }

    /// Whether this property must be present.
    pub fn is_required(&self) -> bool {
        self.lower.unwrap_or(0) > 0
    }
}
