/** Chrono's UTC ordering, with the original wire spelling retained. */
export class ChronoTimestamp {
  readonly wire: string;
  readonly wholeSecond: bigint;
  readonly nanosecond: number;

  private constructor(wire: string, wholeSecond: bigint, nanosecond: number) {
    this.wire = wire;
    this.wholeSecond = wholeSecond;
    this.nanosecond = nanosecond;
    Object.freeze(this);
  }

  static parse(wire: string): ChronoTimestamp {
    // RFC3339 fields, plus Chrono's signed-year serialization extension. Calendar
    // admission belongs to Date's Gregorian implementation, not this lexical adapter.
    const parts = /^([+-]\d{4,6}|\d{4})-(\d{2})-(\d{2})[Tt](\d{2}):(\d{2}):(\d{2})(?:\.(\d+))?([Zz]|[+-]\d{2}:\d{2})$/u.exec(wire);
    if (!parts || parts[0] !== wire) throw new Error("Invalid Chrono timestamp");
    const year = Number(parts[1]), month = Number(parts[2]), day = Number(parts[3]);
    const hour = Number(parts[4]), minute = Number(parts[5]), second = Number(parts[6]);
    const offset = parts[8];
    const offsetHour = offset.length === 1 ? 0 : Number(offset.slice(1, 3));
    const offsetMinute = offset.length === 1 ? 0 : Number(offset.slice(4, 6));
    if (year < -262143 || year > 262142 || month < 1 || month > 12 || day < 1 ||
      hour > 23 || minute > 59 || second > 60 || offsetHour > 23 || offsetMinute > 59) {
      throw new Error("Invalid Chrono timestamp");
    }
    // Avoid Date.UTC's special interpretation of years 0..99. Round-trip all
    // calendar fields before applying the admitted fixed offset; Date rollover
    // cannot turn February 30 or hour 24 into an admitted date.
    const calendar = new Date(0);
    calendar.setUTCFullYear(year, month - 1, day);
    calendar.setUTCHours(hour, minute, second === 60 ? 59 : second, 0);
    if (!Number.isFinite(calendar.getTime()) || calendar.getUTCFullYear() !== year ||
      calendar.getUTCMonth() !== month - 1 || calendar.getUTCDate() !== day ||
      calendar.getUTCHours() !== hour || calendar.getUTCMinutes() !== minute ||
      calendar.getUTCSeconds() !== (second === 60 ? 59 : second)) {
      throw new Error("Invalid Chrono timestamp");
    }
    const offsetSeconds = (offsetHour * 60 + offsetMinute) * 60 * (offset[0] === "-" ? -1 : 1);
    const wholeSecond = BigInt(calendar.getTime() / 1000) - BigInt(offsetSeconds);
    // Chrono's fixed-offset receiver checks the resulting UTC date as well as
    // the local calendar. A boundary-year offset must not escape its UTC range.
    const utcYear = new Date(Number(wholeSecond) * 1000).getUTCFullYear();
    if (!Number.isFinite(utcYear) || utcYear < -262143 || utcYear > 262142) {
      throw new Error("Invalid Chrono timestamp");
    }
    // Chrono admits arbitrary fractional precision and truncates after nine
    // digits. Its leap representation keeps second 59 with nanos >= 1e9.
    const nanosecond = Number((parts[7] ?? "").slice(0, 9).padEnd(9, "0")) +
      (second === 60 ? 1_000_000_000 : 0);
    return new ChronoTimestamp(wire, wholeSecond, nanosecond);
  }

  compare(other: ChronoTimestamp): number {
    if (this.wholeSecond !== other.wholeSecond) return this.wholeSecond < other.wholeSecond ? -1 : 1;
    return Math.sign(this.nanosecond - other.nanosecond);
  }
}
