/** Christmas Eve to New Year's Day, when the app lets it snow. */
export function isChristmas(date: Date): boolean {
  const day = date.getDate();
  return (date.getMonth() === 11 && day >= 24) || (date.getMonth() === 0 && day === 1);
}
