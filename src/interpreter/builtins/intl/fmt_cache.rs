use super::numberformat::{base_locale, grouping_strategy_from_str};
use icu::datetime::DateTimeFormatter;
use icu::datetime::fieldsets::enums::CompositeFieldSet;
use icu::decimal::options::{DecimalFormatterOptions, GroupingStrategy};
use icu::decimal::{DecimalFormatter, DecimalFormatterPreferences};
use icu::locale::Locale as IcuLocale;
use std::cell::RefCell;
use std::rc::Rc;

const FORMATTER_CACHE_CAP: usize = 32;

type Cache<K, V> = RefCell<Vec<(K, Rc<V>)>>;

thread_local! {
    static DECIMAL_FORMATTERS: Cache<(String, GroupingStrategy), DecimalFormatter> =
        const { RefCell::new(Vec::new()) };
    static DATETIME_FORMATTERS: Cache<(IcuLocale, CompositeFieldSet), DateTimeFormatter<CompositeFieldSet>> =
        const { RefCell::new(Vec::new()) };
}

fn get_or_build<K, V>(
    cache: &Cache<K, V>,
    is_hit: impl Fn(&K) -> bool,
    build: impl FnOnce() -> Option<(K, V)>,
) -> Option<Rc<V>> {
    let mut cache = cache.borrow_mut();
    if let Some((_, v)) = cache.iter().find(|(k, _)| is_hit(k)) {
        return Some(v.clone());
    }
    let (key, value) = build()?;
    let value = Rc::new(value);
    if cache.len() >= FORMATTER_CACHE_CAP {
        cache.remove(0);
    }
    cache.push((key, value.clone()));
    Some(value)
}

pub(super) fn decimal_formatter(locale_str: &str, use_grouping: &str) -> Rc<DecimalFormatter> {
    let strategy = grouping_strategy_from_str(use_grouping);
    DECIMAL_FORMATTERS
        .with(|cache| {
            get_or_build(
                cache,
                |(l, g)| l == locale_str && *g == strategy,
                || {
                    let locale: IcuLocale = base_locale(locale_str)
                        .parse()
                        .unwrap_or_else(|_| "en".parse().unwrap());
                    let prefs = DecimalFormatterPreferences::from(&locale);
                    let mut opts = DecimalFormatterOptions::default();
                    opts.grouping_strategy = Some(strategy);
                    let formatter = DecimalFormatter::try_new(prefs, opts).unwrap_or_else(|_| {
                        DecimalFormatter::try_new(Default::default(), opts).unwrap()
                    });
                    Some(((locale_str.to_string(), strategy), formatter))
                },
            )
        })
        .expect("decimal formatter construction falls back to the default locale")
}

pub(super) fn datetime_formatter(
    locale: &IcuLocale,
    field_set: CompositeFieldSet,
) -> Option<Rc<DateTimeFormatter<CompositeFieldSet>>> {
    DATETIME_FORMATTERS.with(|cache| {
        get_or_build(
            cache,
            |(l, fs)| l == locale && *fs == field_set,
            || {
                let formatter =
                    DateTimeFormatter::try_new(locale.clone().into(), field_set).ok()?;
                Some(((locale.clone(), field_set), formatter))
            },
        )
    })
}
