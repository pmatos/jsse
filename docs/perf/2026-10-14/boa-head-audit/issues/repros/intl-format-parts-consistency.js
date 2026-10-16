const locales = ['ar-SA','zh-CN','zh-TW','da-DK','en-US','en-GB','en-CA','en-AU','fr-FR','fr-CA','de-DE','hi-IN','it-IT','ja-JP','ko-KR','pt-BR','pt-PT','ru-RU','es-ES','es-MX','sw-KE','sv-SE','th-TH','tr-TR','vi-VN'];
const styles = ['full','long','medium','short'];
const date = new Date('1800-12-05T12:12:32.000Z');
const rows = [];
for (const locale of locales) for (const dateStyle of styles) for (const timeStyle of styles) for (const utc of [false,true]) {
  const options = {dateStyle,timeStyle};
  if (utc) options.timeZone = 'UTC';
  try {
    const f = new Intl.DateTimeFormat(locale,options);
    const text = f.format(date);
    const parts = f.formatToParts(date);
    rows.push({locale,dateStyle,timeStyle,utc,text,parts,joined:parts.map(p=>p.value).join(''),resolved:f.resolvedOptions()});
  } catch (error) { rows.push({locale,dateStyle,timeStyle,utc,error:String(error)}); }
}
console.log(JSON.stringify({epoch:date.getTime(),supported:Intl.DateTimeFormat.supportedLocalesOf(locales),rows}));
