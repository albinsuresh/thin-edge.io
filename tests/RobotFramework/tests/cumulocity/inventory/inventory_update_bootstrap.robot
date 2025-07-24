*** Settings ***
Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Test Setup          Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:telemetry


*** Test Cases ***
Main device name and type not updated on mapper and agent restart
    Execute Command    tedge connect c8y
    Device Should Exist    ${DEVICE_SN}

    # Change the name directly in the cloud, without using twin topics
    Execute Command    tedge mqtt pub c8y/inventory/managedObjects/update/${DEVICE_SN} '{ "name": "RasPi 0001" }'
    Execute Command    tedge mqtt pub c8y/inventory/managedObjects/update/${DEVICE_SN} '{ "type": "RasPi5" }'

    Execute Command    tedge reconnect c8y
    Sleep    5s

    Device Should Have Fragment Values    name\="RasPi 0001"
    Device Should Have Fragment Values    type\="RasPi5"


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup    connect=${False}
    Set Suite Variable    $DEVICE_SN
