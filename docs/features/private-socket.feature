Feature: The local socket is private without touching the process umask

  Bug: binding set a process-wide umask of 0177 for a moment. A directory any
  other thread created in that moment lost its search bit and was unusable.

  Scenario: The socket is private from the moment it is reachable
    Given no socket exists at the endpoint
    When the app starts listening
    Then the socket at the endpoint has mode 0600
    And it was never reachable at the endpoint with a wider mode

  Scenario: Directories created while the app starts listening stay usable
    Given another thread is creating directories
    When the app starts listening, repeatedly
    Then every directory that thread created can be entered and written to

  Scenario: A stale socket is replaced
    Given a socket file nobody listens on exists at the endpoint
    When the app starts listening
    Then the app listens at the endpoint

  Scenario: A regular file at the endpoint is left alone
    Given a regular file exists at the endpoint
    When the app starts listening
    Then listening fails
    And the file is unchanged

  Scenario: Nothing is left beside the endpoint
    When the app starts listening
    Then no temporary directory remains next to the endpoint
