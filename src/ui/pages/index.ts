// Pages the router loads on demand (administration, charts, data quality,
// forecast, reporting, settings, setup, API reference) are imported by path
// in AppRoutes; exporting them here would pull them into the entry chunk.
export { HomePage } from "./HomePage";
export { AccountPage } from "./AccountPage";
export { ActivationPage } from "./ActivationPage";
export { AuthCallbackPage } from "./AuthCallbackPage";
export { DashboardPage } from "./DashboardPage";
export { ForbiddenPage } from "./ForbiddenPage";
export { LoginPage } from "./LoginPage";
export { RecoveryPage } from "./RecoveryPage";
export { SystemLayoutPage } from "./SystemLayoutPage";
