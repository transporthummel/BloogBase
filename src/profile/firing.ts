// The setting and every profile's status share the prefix, so changing the
// setting refreshes what the game pages say.
export const oneButtonFiringKey = ["firing"] as const;
export const profileFiringKey = (profileId: string) => ["firing", profileId] as const;
