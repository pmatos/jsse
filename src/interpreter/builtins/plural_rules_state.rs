use crate::interpreter::types::{IntlData, JsObjectData};
use crate::types::JsValue;
use icu::locale::Locale as IcuLocale;
use icu::plurals::{
    PluralCategory, PluralRuleType, PluralRules as IcuPluralRules,
    PluralRulesOptions as IcuPluralRulesOptions, PluralRulesPreferences, PluralRulesWithRanges,
};
use std::cell::OnceCell;
use std::rc::Rc;

/// The immutable option snapshot of an `Intl.PluralRules` instance (ECMA-402 §16.1).
/// `locale` is already stripped of Unicode extensions by the constructor.
pub(crate) struct PluralRulesOptions {
    pub(crate) locale: String,
    pub(crate) plural_type: String,
    pub(crate) notation: String,
    /// [[CompactDisplay]] — Some only when notation is "compact" (§17.4).
    pub(crate) compact_display: Option<String>,
    pub(crate) minimum_integer_digits: u32,
    pub(crate) minimum_fraction_digits: u32,
    pub(crate) maximum_fraction_digits: u32,
    pub(crate) minimum_significant_digits: Option<u32>,
    pub(crate) maximum_significant_digits: Option<u32>,
    pub(crate) rounding_mode: String,
    pub(crate) rounding_increment: u32,
    pub(crate) rounding_priority: String,
    pub(crate) trailing_zero_display: String,
}

/// An instance's options plus the ICU rule objects derived from its immutable
/// `locale` and `type`, built on first use and then shared by every call.
pub(crate) struct PluralRulesState {
    pub(crate) options: PluralRulesOptions,
    rules: OnceCell<Option<IcuPluralRules>>,
    range_rules: OnceCell<Option<PluralRulesWithRanges<IcuPluralRules>>>,
}

/// ICU inputs for a locale already stripped of Unicode extensions.
pub(crate) fn icu_inputs(
    locale: &str,
    plural_type: &str,
) -> (PluralRulesPreferences, IcuPluralRulesOptions) {
    let icu_locale: IcuLocale = locale.parse().unwrap_or_else(|_| "en".parse().unwrap());
    let mut opts = IcuPluralRulesOptions::default();
    opts.rule_type = Some(if plural_type == "ordinal" {
        PluralRuleType::Ordinal
    } else {
        PluralRuleType::Cardinal
    });
    (PluralRulesPreferences::from(&icu_locale), opts)
}

impl PluralRulesState {
    /// The state of a PluralRules object; `None` for any other object kind.
    pub(crate) fn of(data: &JsObjectData) -> Option<Rc<Self>> {
        match data.intl_data() {
            Some(IntlData::PluralRules(state)) => Some(Rc::clone(state)),
            _ => None,
        }
    }

    pub(crate) fn new(options: PluralRulesOptions) -> Rc<Self> {
        Rc::new(Self {
            options,
            rules: OnceCell::new(),
            range_rules: OnceCell::new(),
        })
    }

    /// `None` when ICU has no rules for the locale; callers answer "other".
    pub(crate) fn rules(&self) -> Option<&IcuPluralRules> {
        self.rules
            .get_or_init(|| {
                let (prefs, opts) = icu_inputs(&self.options.locale, &self.options.plural_type);
                IcuPluralRules::try_new(prefs, opts).ok()
            })
            .as_ref()
    }

    pub(crate) fn range_rules(&self) -> Option<&PluralRulesWithRanges<IcuPluralRules>> {
        self.range_rules
            .get_or_init(|| {
                let (prefs, opts) = icu_inputs(&self.options.locale, &self.options.plural_type);
                PluralRulesWithRanges::try_new(prefs, opts).ok()
            })
            .as_ref()
    }
}

/// Categories in ECMA-402 `pluralCategories` order; `CATEGORY_NAMES` is indexed alike.
pub(crate) const SPEC_ORDER: [PluralCategory; 6] = [
    PluralCategory::Zero,
    PluralCategory::One,
    PluralCategory::Two,
    PluralCategory::Few,
    PluralCategory::Many,
    PluralCategory::Other,
];

const CATEGORY_NAMES: [&str; 6] = ["zero", "one", "two", "few", "many", "other"];

fn category_index(cat: PluralCategory) -> usize {
    match cat {
        PluralCategory::Zero => 0,
        PluralCategory::One => 1,
        PluralCategory::Two => 2,
        PluralCategory::Few => 3,
        PluralCategory::Many => 4,
        PluralCategory::Other => 5,
    }
}

pub(crate) fn category_name(cat: PluralCategory) -> &'static str {
    CATEGORY_NAMES[category_index(cat)]
}

thread_local! {
    static CATEGORY_VALUES: [JsValue; 6] = CATEGORY_NAMES.map(JsValue::from_str);
}

/// The category's string value, shared per thread instead of allocated per call.
pub(crate) fn category_value(cat: PluralCategory) -> JsValue {
    CATEGORY_VALUES.with(|values| values[category_index(cat)].clone())
}

pub(crate) fn other_value() -> JsValue {
    category_value(PluralCategory::Other)
}
