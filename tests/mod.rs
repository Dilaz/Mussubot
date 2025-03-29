mod smoke_tests;
mod google_calendar_mock;
mod redis_mock;

// This file organizes the integration tests into a cohesive test suite.
// Each module tests a specific aspect of the application:
// - smoke_tests: Basic functionality tests to ensure nothing is broken
// - google_calendar_mock: Mocking the Google Calendar API for testing
// - redis_mock: Mocking Redis for testing without a real Redis instance 