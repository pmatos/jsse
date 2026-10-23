use super::{base_locale, grouping_strategy_from_str};
use icu::datetime::DateTimeFormatter;
use icu::datetime::fieldsets::enums::CompositeFieldSet;
use icu::decimal::options::DecimalFormatterOptions;
use icu::decimal::{DecimalFormatter, DecimalFormatterPreferences};
use icu::locale::Locale as IcuLocale;
use std::cell::RefCell;
use std::rc::Rc;

const FORMATTER_CACHE_CAP: usize = 32;

thread_local! {
    static DECIMAL_FORMATTERS: RefCell<Vec<(String, String, Rc<DecimalFormatter>)>> =
        const { RefCell::new(Vec::new()) };
    static DATETIME_FORMATTERS: RefCell<
        Vec<(IcuLocale, CompositeFieldSet, Rc<DateTimeFormatter<CompositeFieldSet>>)>,
    > = const { RefCell::new(Vec::new()) };
}

pub(in crate::interpreter::builtins::intl) fn decimal_formatter(
    locale_str: &str,
    use_grouping: &str,
) -> Rc<DecimalFormatter> {
    DECIMAL_FORMATTERS.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((_, _, f)) = cache
            .iter()
            .find(|(l, g, _)| l == locale_str && g == use_grouping)
        {
            return f.clone();
        }
        let base = base_locale(locale_str);
        let locale: IcuLocale = base.parse().unwrap_or_else(|_| "en".parse().unwrap());
        let prefs = DecimalFormatterPreferences::from(&locale);
        let mut opts = DecimalFormatterOptions::default();
        opts.grouping_strategy = Some(grouping_strategy_from_str(use_grouping));
        let formatter = Rc::new(
            DecimalFormatter::try_new(prefs, opts)
                .unwrap_or_else(|_| DecimalFormatter::try_new(Default::default(), opts).unwrap()),
        );
        if cache.len() >= FORMATTER_CACHE_CAP {
            cache.remove(0);
        }
        cache.push((
            locale_str.to_string(),
            use_grouping.to_string(),
            formatter.clone(),
        ));
        formatter
    })
}

pub(in crate::interpreter::builtins::intl) fn datetime_formatter(
    locale: &IcuLocale,
    field_set: CompositeFieldSet,
) -> Option<Rc<DateTimeFormatter<CompositeFieldSet>>> {
    DATETIME_FORMATTERS.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((_, _, f)) = cache
            .iter()
            .find(|(l, fs, _)| l == locale && *fs == field_set)
        {
            return Some(f.clone());
        }
        let formatter = Rc::new(DateTimeFormatter::try_new(locale.clone().into(), field_set).ok()?);
        if cache.len() >= FORMATTER_CACHE_CAP {
            cache.remove(0);
        }
        cache.push((locale.clone(), field_set, formatter.clone()));
        Some(formatter)
    })
}
