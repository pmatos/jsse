export const info = {
  main: import.meta.main,
  file: import.meta.filename.split('/').pop(),
  dir: import.meta.dirname.split('/').pop(),
  url: import.meta.url.split('/').pop(),
};
