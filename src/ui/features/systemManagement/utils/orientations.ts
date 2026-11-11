/** The eight compass directions offered for a string, as azimuth and label key. */
export const ORIENTATIONS = [
  [0, "north"],
  [45, "northEast"],
  [90, "east"],
  [135, "southEast"],
  [180, "south"],
  [225, "southWest"],
  [270, "west"],
  [315, "northWest"],
] as const;
