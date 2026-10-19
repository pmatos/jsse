use crate::types::JsValue;
use icu::locale::Locale as IcuLocale;
use icu::plurals::{
    PluralCategory, PluralRuleType, PluralRules as IcuPluralRules,
    PluralRulesOptions as IcuPluralRulesOptions, PluralRulesPreferences, PluralRulesWithRanges,
};
use std::cell::OnceCell;

/// The immutable option snapshot of an `Intl.PluralRules` instance (ECMA-402 §16.1).
/// `locale` is already stripped of Unicode extensions by the constructor.
#[derive(Clone)]
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

fn icu_inputs(options: &PluralRulesOptions) -> (PluralRulesPreferences, IcuPluralRulesOptions) {
    let icu_locale: IcuLocale = options
        .locale
        .parse()
        .unwrap_or_else(|_| "en".parse().unwrap());
    let mut opts = IcuPluralRulesOptions::default();
    opts.rule_type = Some(if options.plural_type == "ordinal" {
        PluralRuleType::Ordinal
    } else {
        PluralRuleType::Cardinal
    });
    (PluralRulesPreferences::from(&icu_locale), opts)
}

impl PluralRulesState {
    pub(crate) fn new(options: PluralRulesOptions) -> Self {
        Self {
            options,
            rules: OnceCell::new(),
            range_rules: OnceCell::new(),
        }
    }

    /// `None` when ICU has no rules for the locale; callers answer "other".
    pub(crate) fn rules(&self) -> Option<&IcuPluralRules> {
        self.rules
            .get_or_init(|| {
                let (prefs, opts) = icu_inputs(&self.options);
                IcuPluralRules::try_new(prefs, opts).ok()
            })
            .as_ref()
    }

    pub(crate) fn range_rules(&self) -> Option<&PluralRulesWithRanges<IcuPluralRules>> {
        self.range_rules
            .get_or_init(|| {
                let (prefs, opts) = icu_inputs(&self.options);
                PluralRulesWithRanges::try_new(prefs, opts).ok()
            })
            .as_ref()
    }
}

thread_local! {
    static CATEGORY_VALUES: [JsValue; 6] = [
        JsValue::from_str("zero"),
        JsValue::from_str("one"),
        JsValue::from_str("two"),
        JsValue::from_str("few"),
        JsValue::from_str("many"),
        JsValue::from_str("other"),
    ];
}

/// The category's string value, shared per thread instead of allocated per call.
pub(crate) fn category_value(cat: PluralCategory) -> JsValue {
    let idx = match cat {
        PluralCategory::Zero => 0,
        PluralCategory::One => 1,
        PluralCategory::Two => 2,
        PluralCategory::Few => 3,
        PluralCategory::Many => 4,
        PluralCategory::Other => 5,
    };
    CATEGORY_VALUES.with(|values| values[idx].clone())
}

pub(crate) fn other_value() -> JsValue {
    category_value(PluralCategory::Other)
}
