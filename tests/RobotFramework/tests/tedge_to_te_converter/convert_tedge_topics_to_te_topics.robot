*** Settings ***
Documentation       Purpose of this test is to verify that tedge-agent converts the tedge/# topics to te/# topics

Resource            ../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Custom Setup
Suite Teardown      Custom Teardown

Test Tags           theme:mqtt    theme:tedge to te

Library             JSONLibrary


*** Test Cases ***
Convert main device measurement topic
    Execute Command    tedge mqtt pub tedge/measurements '{"temperature":25}'
    Should Have MQTT Messages    te/device/main///m/    message_pattern={"temperature":25}


Convert child device measurement topic
    Execute Command    tedge mqtt pub tedge/measurements/child '{"temperature":25}'
    Should Have MQTT Messages    te/device/child///m/    message_pattern={"temperature":25}

Convert main device event topic
    Execute Command    tedge mqtt pub tedge/events/login_event '{"text":"someone logedin"}'
    Should Have MQTT Messages    te/device/main///e/login_event    message_pattern={"text":"someone logedin"}

Convert child device event topic
    Execute Command    tedge mqtt pub tedge/events/login_event/child '{"text":"someone logedin"}'
    Should Have MQTT Messages    te/device/child///e/login_event    message_pattern={"text":"someone logedin"}

Convert main device alarm topic
    Execute Command    tedge mqtt pub tedge/alarms/minor/test_alarm '{"text":"test alarm"}' -q 2 -r
    ${messages}=    Should Have MQTT Messages    te/device/main///a/test_alarm    minimum=1    maximum=1
    ${message}=    Convert String To Json    ${messages[0]}
    Should Be Equal    ${message["severity"]}    minor

Convert main device alarm topic and retain
    Execute Command    tedge mqtt pub tedge/alarms/minor/test_alarm '{"text":"test alarm"}' -q 2 -r
    ${messages}=    Should Have MQTT Messages    te/device/main///a/test_alarm    minimum=1     maximum=1
    ${message}=    Convert String To Json    ${messages[0]}
    Should Be Equal    ${message["severity"]}    minor
    # Check if the retained message received with new client or not
    ${result}=    Execute Command    tedge mqtt sub te/device/main///a/test_alarm & sleep 2s; kill $!   
    Should Contain    ${result}    "severity":"minor"

Convert child device alarm topic
    Execute Command    tedge mqtt pub tedge/alarms/major/test_alarm/child '{"text":"test alarm"}' -q 2 -r
    ${messages}=    Should Have MQTT Messages    te/device/child///a/test_alarm    minimum=1     maximum=1
    ${message}=    Convert String To Json    ${messages[0]}
    Should Be Equal    ${message["severity"]}    major
   

Convert main device service health topic
    Execute Command    tedge mqtt pub tedge/health/main-service '{"pid":1234,"status":"up"}' -q 2 -r
    ${messages}=    Should Have MQTT Messages    te/device/main/service/main-service/status/health    minimum=1     maximum=1
    ${message}=    Convert String To Json    ${messages[0]}
    Should Be Equal As Numbers    ${message["pid"]}    1234
    Should Be Equal    ${message["status"]}    up


Convert child device service health topic
    Execute Command    tedge mqtt pub tedge/health/child/child-service '{"pid":1234,"status":"up"}' -q 2 -r
    ${messages}=    Should Have MQTT Messages    te/device/child/service/child-service/status/health    minimum=1     maximum=1
    ${message}=    Convert String To Json    ${messages[0]}
    Should Be Equal As Numbers    ${message["pid"]}    1234
    Should Be Equal    ${message["status"]}    up

Convert main device service health topic and retain
    Execute Command    tedge mqtt pub tedge/health/main-service '{"pid":1234,"status":"up"}' -q 2 -r
    ${messages}=    Should Have MQTT Messages    te/device/main/service/main-service/status/health    minimum=1     maximum=1
    ${message}=    Convert String To Json    ${messages[0]}
    Should Be Equal As Numbers    ${message["pid"]}    1234
    Should Be Equal    ${message["status"]}    up
    # Check if the retained message received with new client or not
    ${result}=    Execute Command    tedge mqtt sub te/device/main/service/main-service/status/health & sleep 2s; kill $!
    Should Contain    ${result}    "pid":1234
    Should Contain    ${result}    "status":"up"

*** Keywords ***
Custom Setup
    Setup
    ThinEdgeIO.Service Health Status Should Be Up    tedge-agent

Custom Teardown
    Get Logs