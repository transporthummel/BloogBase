import logo from "../assets/logo.svg";
import logoLight from "../assets/logo-light.svg";

/**
 * The logo with its wordmark, in the colours of the current theme: light
 * letters on Midnight Galaxy, dark ones on Vintage Beige.
 */
export function Logo({ alt = "BloogBase", className = "" }: { alt?: string; className?: string }) {
  return (
    <>
      <img src={logo} alt={alt} width={677} height={175} className={`${className} light:hidden`} />
      <img
        src={logoLight}
        alt={alt}
        width={677}
        height={175}
        className={`${className} hidden light:block`}
      />
    </>
  );
}
