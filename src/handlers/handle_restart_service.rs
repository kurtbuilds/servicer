use crate::{
    handlers::handle_show_status::handle_show_status,
    utils::{
        service_actions::{start_service, stop_service}, 
        service_names::get_full_service_name, 
        systemd::ManagerProxy,
    },
};

/// Restarts a service by stopping it and then starting it
///
/// # Arguments
///
/// * `name` - Name of the service to restart  
/// * `show_status` - Whether to show status after restart
///
pub async fn handle_restart_service(
    name: &String,
    show_status: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let full_service_name = get_full_service_name(&name);

    let connection = zbus::Connection::system().await?;
    let manager_proxy = ManagerProxy::new(&connection).await?;
    
    // Stop the service first
    stop_service(&manager_proxy, &full_service_name).await;
    println!("Stopped {name}");
    
    // Then start it
    start_service(&manager_proxy, &full_service_name).await;
    println!("Started {name}");

    if show_status {
        handle_show_status().await?;
    }

    Ok(())
}